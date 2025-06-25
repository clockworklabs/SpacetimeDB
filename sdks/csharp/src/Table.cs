using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using SpacetimeDB.EventHandling;

#nullable enable
namespace SpacetimeDB
{
    public abstract class RemoteBase
    {
        protected readonly IDbConnection conn;

        protected RemoteBase(IDbConnection conn)
        {
            this.conn = conn;
        }
    }

    public interface IRemoteTableHandle
    {
        internal object? GetPrimaryKey(IStructuralReadWrite row);
        internal string RemoteTableName { get; }

        internal Type ClientTableType { get; }
        internal PreHashedRow DecodeValue(BinaryReader reader);

        /// <summary>
        /// Start applying a delta to the table.
        /// This is called for all tables before any updates are actually applied, allowing OnBeforeDelete to be invoked correctly.
        /// </summary>
        /// <param name="multiDictionaryDelta"></param>
        internal void PreApply(IEventContext context, MultiDictionaryDelta<object, PreHashedRow> multiDictionaryDelta);

        /// <summary>
        /// Apply a delta to the table.
        /// Should not invoke any user callbacks, since not all tables have been updated yet.
        /// Should fix up indices, to be ready for PostApply.
        /// </summary>
        /// <param name="multiDictionaryDelta"></param>
        internal void Apply(IEventContext context, MultiDictionaryDelta<object, PreHashedRow> multiDictionaryDelta);

        /// <summary>
        /// Finish applying a delta to a table.
        /// This is when row callbacks (besides OnBeforeDelete) actually happen.
        /// </summary>
        internal void PostApply(IEventContext context);
    }


    /// <summary>
    /// Base class for views of remote tables.
    /// </summary>
    /// <typeparam name="EventContext"></typeparam>
    /// <typeparam name="Row"></typeparam>
    public abstract class RemoteTableHandle<EventContext, Row> : RemoteBase, IRemoteTableHandle
        where EventContext : class, IEventContext
        where Row : class, IStructuralReadWrite, new()
    {
        // Note: This should really be also parameterized with RowRW: IReadWrite<Row>, but that is a backwards-
        // incompatible change. Instead, we call (IReadWrite<Row>)((IStructuralReadWrite)new Row()).GetSerializer().
        // Serializer.Read is faster than IStructuralReadWrite.Read<Row> since it's manually monomorphized
        // and therefore avoids using reflection when initializing the row object.

        public abstract class IndexBase<Column>
            where Column : IEquatable<Column>
        {
            protected abstract Column GetKey(Row row);
        }

        public abstract class UniqueIndexBase<Column> : IndexBase<Column>
            where Column : IEquatable<Column>
        {
            // This is not typed, to avoid the runtime overhead of generics.
            // Despite that, every preHashedRow.Row in this cache is guaranteed to be of type Row.
            private readonly Dictionary<Column, PreHashedRow> cache = new();

            public UniqueIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += rows =>
                {
                    foreach (var preHashed in rows)
                    {
                        // Guaranteed to be a valid cast by contract of OnInternalInsert.
                        cache.Add(GetKey((Row)preHashed.Row), preHashed);
                    }
                };
                table.OnInternalDelete += rows =>
                {
                    foreach (var preHashed in rows)
                    {
                        // Guaranteed to be a valid cast by contract of OnInternalDelete.
                        cache.Remove(GetKey((Row)preHashed.Row));
                    }
                };
            }

            public Row? Find(Column value) => cache.TryGetValue(value, out var row) ? (Row)row.Row : null;
        }

        public abstract class BTreeIndexBase<Column> : IndexBase<Column>
            where Column : IEquatable<Column>, IComparable<Column>
        {
            // TODO: change to SortedDictionary when adding support for range queries.
            private readonly Dictionary<Column, SmallHashSetOfPreHashedRow> cache = new();

            public BTreeIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += preHashedRows =>
                {
                    foreach (var preHashed in preHashedRows)
                    {
                        // Guaranteed to be a valid cast by contract of OnInternalInsert.
                        var row = (Row)preHashed.Row;
                        var key = GetKey(row);
                        if (cache.TryGetValue(key, out var set))
                        {
                            set.Add(preHashed);
                            // Need to update the parent dictionary: `set` is a mutable struct.
                            // Just updating the local `set` variable won't update the parent dict.
                            cache[key] = set;
                        }
                        else
                        {
                            set = new()
                            {
                                preHashed
                            };
                            cache.Add(key, set);
                        }
                    }
                };

                table.OnInternalDelete += preHashedRows =>
                {
                    foreach (var preHashed in preHashedRows)
                    {
                        // Guaranteed to be a valid cast by contract of OnInternalDelete.
                        var row = (Row)preHashed.Row;
                        var key = GetKey(row);
                        var set = cache[key];
                        set.Remove(preHashed);
                        if (set.Count == 0)
                        {
                            cache.Remove(key);
                        }
                        else
                        {
                            // Need to update the parent dictionary: `set` is a mutable struct.
                            // Just updating the local `set` variable won't update the parent dict.
                            cache[key] = set;
                        }
                    }
                };
            }

            public IEnumerable<Row> Filter(Column value) =>
                cache.TryGetValue(value, out var rows) ? rows.Select(preHashed => (Row)preHashed.Row) : Enumerable.Empty<Row>();
        }


        protected abstract string RemoteTableName { get; }
        string IRemoteTableHandle.RemoteTableName => RemoteTableName;

        public RemoteTableHandle(IDbConnection conn) : base(conn) { }

        // This method needs to be overridden by autogen.
        protected virtual object? GetPrimaryKey(Row row) => null;

        // These events are used by indices to add/remove rows to their dictionaries.
        //
        // They are passed all the modified rows for an update at once:
        // this avoids the overhead of invoking handlers per-row.
        // (Unfortunately, it's too late to make this sort of change for user callbacks...)
        //
        // These callbacks can assume the Row stored in the PreHashedRow passed is of the correct type;
        // the check is done before performing these callbacks.
        private AbstractEventHandler<List<PreHashedRow>> OnInternalInsertHandler { get; } = new();
        private event Action<List<PreHashedRow>> OnInternalInsert
        {
            add => OnInternalInsertHandler.AddListener(value);
            remove => OnInternalInsertHandler.RemoveListener(value);
        }
        private AbstractEventHandler<List<PreHashedRow>> OnInternalDeleteHandler { get; } = new();
        private event Action<List<PreHashedRow>> OnInternalDelete
        {
            add => OnInternalDeleteHandler.AddListener(value);
            remove => OnInternalDeleteHandler.RemoveListener(value);
        }

        // These are implementations of the type-erased interface.
        object? IRemoteTableHandle.GetPrimaryKey(IStructuralReadWrite row) => GetPrimaryKey((Row)row);

        // These are provided by RemoteTableHandle.
        Type IRemoteTableHandle.ClientTableType => typeof(Row);

        // THE DATA IN THE TABLE.
        // The keys of this map are:
        // - Primary keys, if we have them.
        // - The entire row itself, if we don't.
        // But really, the keys are whatever SpacetimeDBClient chooses to give us.
        private readonly MultiDictionary<object, PreHashedRow> Entries = new(EqualityComparer<object>.Default, PreHashedRowComparer.Default);

        private static IReadWrite<Row>? _serializer;

        /// <summary>
        /// Serializer for the rows of this table.
        /// </summary>
        private static IReadWrite<Row> Serializer
        {
            get
            {
                // We can't just initialize this statically, because some BitCraft row types have static
                // methods that read SpacetimeDBService.Conn.Db, and these fail if the connection is not
                // there on the first load of those types (????).
                // This should really be considered an error on their part, but for now we delay initializing any Rows until
                // Serializer is actually read, that is, until a row actually needs to be deserialized --
                // at which point, the connection should be initialized.
                if (_serializer == null)
                {
                    _serializer = (IReadWrite<Row>)new Row().GetSerializer();
                }
                return _serializer;
            }
        }

        // The function to use for decoding a type value.
        PreHashedRow IRemoteTableHandle.DecodeValue(BinaryReader reader) => new PreHashedRow(Serializer.Read(reader));

        public delegate void RowEventHandler(EventContext context, Row row);
        private CustomRowEventHandler OnInsertHandler { get; } = new();
        public event RowEventHandler OnInsert
        {
            add => OnInsertHandler.AddListener(value);
            remove => OnInsertHandler.RemoveListener(value);
        }
        private CustomRowEventHandler OnDeleteHandler { get; } = new();
        public event RowEventHandler OnDelete
        {
            add => OnDeleteHandler.AddListener(value);
            remove => OnDeleteHandler.RemoveListener(value);
        }
        private CustomRowEventHandler OnBeforeDeleteHandler { get; } = new();
        public event RowEventHandler OnBeforeDelete
        {
            add => OnBeforeDeleteHandler.AddListener(value);
            remove => OnBeforeDeleteHandler.RemoveListener(value);
        }

        public delegate void UpdateEventHandler(EventContext context, Row oldRow, Row newRow);
        private CustomUpdateEventHandler OnUpdateHandler { get; } = new();
        public event UpdateEventHandler OnUpdate
        {
            add => OnUpdateHandler.AddListener(value);
            remove => OnUpdateHandler.RemoveListener(value);
        }

        public int Count => (int)Entries.CountDistinct;

        public IEnumerable<Row> Iter() => Entries.Entries.Select(entry => (Row)entry.Value.Row);

        public Task<Row[]> RemoteQuery(string query) =>
            conn.RemoteQuery<Row>($"SELECT {RemoteTableName}.* FROM {RemoteTableName} {query}");

        void InvokeInsert(IEventContext context, IStructuralReadWrite row)
        {
            try
            {
                OnInsertHandler.Invoke((EventContext)context, (Row)row);
            }
            catch (Exception e)
            {
                Log.Exception(e);
            }
        }

        void InvokeDelete(IEventContext context, IStructuralReadWrite row)
        {
            try
            {
                OnDeleteHandler.Invoke((EventContext)context, (Row)row);
            }
            catch (Exception e)
            {
                Log.Exception(e);
            }
        }

        void InvokeBeforeDelete(IEventContext context, IStructuralReadWrite row)
        {
            try
            {
                OnBeforeDeleteHandler.Invoke((EventContext)context, (Row)row);
            }
            catch (Exception e)
            {
                Log.Exception(e);
            }
        }

        void InvokeUpdate(IEventContext context, IStructuralReadWrite oldRow, IStructuralReadWrite newRow)
        {
            try
            {
                OnUpdateHandler.Invoke((EventContext)context, (Row)oldRow, (Row)newRow);
            }
            catch (Exception e)
            {
                Log.Exception(e);
            }
        }

        List<KeyValuePair<object, PreHashedRow>> wasInserted = new();
        List<(object key, PreHashedRow oldValue, PreHashedRow newValue)> wasUpdated = new();
        List<KeyValuePair<object, PreHashedRow>> wasRemoved = new();

        void IRemoteTableHandle.PreApply(IEventContext context, MultiDictionaryDelta<object, PreHashedRow> multiDictionaryDelta)
        {
            Debug.Assert(wasInserted.Count == 0 && wasUpdated.Count == 0 && wasRemoved.Count == 0, "Call Apply and PostApply before calling PreApply again");

            foreach (var (_, value) in Entries.WillRemove(multiDictionaryDelta))
            {
                InvokeBeforeDelete(context, value.Row);
            }
        }

        List<PreHashedRow> scratchInsertBuffer = new();
        List<PreHashedRow> scratchDeleteBuffer = new();

        void IRemoteTableHandle.Apply(IEventContext context, MultiDictionaryDelta<object, PreHashedRow> multiDictionaryDelta)
        {
            try
            {
                Entries.Apply(multiDictionaryDelta, wasInserted, wasUpdated, wasRemoved);
            }
            catch (Exception e)
            {
                var deltaString = multiDictionaryDelta.ToString();
                deltaString = deltaString[..Math.Min(deltaString.Length, 10_000)];
                var entriesString = Entries.ToString();
                entriesString = entriesString[..Math.Min(entriesString.Length, 10_000)];
                throw new Exception($"While table `{RemoteTableName}` was applying:\n{deltaString} \nto:\n{entriesString}", e);
            }

            // I would like to pre-allocate the needed capacity in scratchInsertBuffer here, but there is no way to :(
            scratchInsertBuffer.Clear();

            // Update indices.
            // This is a local operation -- it only looks at our indices and doesn't invoke user code.
            // So we don't need to wait for other tables to be updated to do it.
            // (And we need to do it before ANY PostApply is called.)

            // First, we gather the updated rows into buffers.
            // This is also when we verify that the PreHashedRows store rows of the correct types.
            foreach (var (_, value) in wasInserted)
            {
                if (value.Row is Row newRow)
                {
                    scratchInsertBuffer.Add(value);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {value.GetType().Name}");
                }
            }
            // Then, we dispatch the index update handlers on the buffers.
            // Doing things a buffer-at-a-time lets us avoid indirection in the inner loop.
            OnInternalInsertHandler.Invoke(scratchInsertBuffer);

            scratchInsertBuffer.Clear();
            scratchDeleteBuffer.Clear();

            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                if (oldValue.Row is Row oldRow)
                {
                    scratchDeleteBuffer.Add(oldValue);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {oldValue.GetType().Name}");
                }
                if (newValue.Row is Row newRow)
                {
                    scratchInsertBuffer.Add(newValue);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {newValue.GetType().Name}");
                }
            }

            // Make sure to invoke OnInternalDeleteHandler first, otherwise
            // we might accidentally delete a row after we update it!
            OnInternalDeleteHandler.Invoke(scratchDeleteBuffer);
            OnInternalInsertHandler.Invoke(scratchInsertBuffer);

            scratchInsertBuffer.Clear();
            scratchDeleteBuffer.Clear();

            foreach (var (_, value) in wasRemoved)
            {
                if (value.Row is Row oldRow)
                {
                    scratchDeleteBuffer.Add(value);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {value.GetType().Name}");
                }
            }
            OnInternalDeleteHandler.Invoke(scratchDeleteBuffer);

            scratchDeleteBuffer.Clear();
        }

        void IRemoteTableHandle.PostApply(IEventContext context)
        {
            foreach (var (_, value) in wasInserted)
            {
                InvokeInsert(context, value.Row);
            }
            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                InvokeUpdate(context, oldValue.Row, newValue.Row);
            }
            foreach (var (_, value) in wasRemoved)
            {
                InvokeDelete(context, value.Row);
            }
            wasInserted.Clear();
            wasUpdated.Clear();
            wasRemoved.Clear();

        }

        private class CustomRowEventHandler
        {
            private EventListeners<RowEventHandler> Listeners { get; } = new();

            public void Invoke(EventContext ctx, Row row)
            {
                for (var i = Listeners.Count - 1; i >= 0; i--)
                {
                    Listeners[i]?.Invoke(ctx, row);
                }
            }

            public void AddListener(RowEventHandler listener) => Listeners.Add(listener);
            public void RemoveListener(RowEventHandler listener) => Listeners.Remove(listener);
        }
        private class CustomUpdateEventHandler
        {
            private EventListeners<UpdateEventHandler> Listeners { get; } = new();

            public void Invoke(EventContext ctx, Row oldRow, Row newRow)
            {
                for (var i = Listeners.Count - 1; i >= 0; i--)
                {
                    Listeners[i]?.Invoke(ctx, oldRow, newRow);
                }
            }

            public void AddListener(UpdateEventHandler listener) => Listeners.Add(listener);
            public void RemoveListener(UpdateEventHandler listener) => Listeners.Remove(listener);
        }
    }
}

/// <summary>
/// An immutable row, with its hash precomputed.
/// Inserting values into indexes on the main thread requires a lot of hashing, and for large rows,
/// this takes a lot of time.
/// Pre-computing the hash saves main thread time.
/// It costs time on the preprocessing thread, but hopefully that thread is less loaded.
/// Also, once we parallelize message pre-processing, we can split this work over a thread pool.
/// 
/// You MUST create objects of this type with the single-argument constructor.
/// Default-initializing an object of this type breaks its invariant, which is that Hash is the hash of Row.
/// 
/// Note: if struct types for table rows are ever added to the SDK, this should be updated to take a generic argument
/// of type Row. This *may* be able to avoid boxing the row structs -- you currently have to box them, since they're stored type-erased
/// as IStructuralReadWrite. But it doesn't matter rn, since currently all row types are generated as classes.
/// This may be challenging to massage SpacetimeDBClient into supporting, though.
/// </summary>
internal struct PreHashedRow
{
    /// <summary>
    /// The row itself.
    /// Mutating this value breaks the invariant of this type.
    /// Mutations should be impossible in our workflow, but you never know.
    /// </summary>
    public readonly IStructuralReadWrite Row;

    /// <summary>
    /// The hash of the row.
    /// </summary>
    readonly int Hash;

    public PreHashedRow(IStructuralReadWrite Row)
    {
        this.Row = Row;
        Hash = Row.GetHashCode();
    }

    public override int GetHashCode()
    {
        return Hash;
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    public bool Equals(PreHashedRow other)
        // compare hashes too: speeds up if not equal, not expensive if they are equal.
        => Hash == other.Hash && Row.Equals(other.Row);

    public override bool Equals(object? other)
    {
        if (other == null)
        {
            return false; // it is impossible for Row to be null
        }
        var other_ = other as PreHashedRow?;
        if (other_ == null)
        {
            return false;
        }
        return Equals(other_.Value);
    }

    public override string ToString()
        => Row.ToString();
}

internal class PreHashedRowComparer : IEqualityComparer<PreHashedRow>
{
    public static PreHashedRowComparer Default = new();

    public bool Equals(PreHashedRow x, PreHashedRow y)
    {
        return x.Equals(y);
    }

    public int GetHashCode(PreHashedRow obj)
    {
        return obj.GetHashCode();
    }
}

#nullable disable
