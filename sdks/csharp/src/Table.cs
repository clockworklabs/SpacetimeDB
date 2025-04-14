using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;

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
        internal IStructuralReadWrite DecodeValue(byte[] bytes);

        /// <summary>
        /// Start applying a delta to the table.
        /// This is called for all tables before any updates are actually applied, allowing OnBeforeDelete to be invoked correctly.
        /// </summary>
        /// <param name="multiDictionaryDelta"></param>
        internal void PreApply(IEventContext context, MultiDictionaryDelta<object, DbValue> multiDictionaryDelta);

        /// <summary>
        /// Apply a delta to the table.
        /// Should not invoke any user callbacks, since not all tables have been updated yet.
        /// Should fix up indices, to be ready for PostApply.
        /// </summary>
        /// <param name="multiDictionaryDelta"></param>
        internal void Apply(IEventContext context, MultiDictionaryDelta<object, DbValue> multiDictionaryDelta);

        /// <summary>
        /// Finish applying a delta to a table.
        /// This is when row callbacks (besides OnBeforeDelete) actually happen.
        /// </summary>
        internal void PostApply(IEventContext context);
    }


    public abstract class RemoteTableHandle<EventContext, Row> : RemoteBase, IRemoteTableHandle
        where EventContext : class, IEventContext
        where Row : class, IStructuralReadWrite, new()
    {
        public abstract class IndexBase<Column>
            where Column : IEquatable<Column>
        {
            protected abstract Column GetKey(Row row);
        }

        public abstract class UniqueIndexBase<Column> : IndexBase<Column>
            where Column : IEquatable<Column>
        {
            private readonly Dictionary<Column, Row> cache = new();

            public UniqueIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row => cache.Add(GetKey(row), row);
                table.OnInternalDelete += row => cache.Remove(GetKey(row));
            }

            public Row? Find(Column value) => cache.TryGetValue(value, out var row) ? row : null;
        }

        public abstract class BTreeIndexBase<Column> : IndexBase<Column>
            where Column : IEquatable<Column>, IComparable<Column>
        {
            // TODO: change to SortedDictionary when adding support for range queries.
            private readonly Dictionary<Column, HashSet<Row>> cache = new();

            public BTreeIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row =>
                {
                    var key = GetKey(row);
                    if (!cache.TryGetValue(key, out var rows))
                    {
                        rows = new();
                        cache.Add(key, rows);
                    }
                    rows.Add(row);
                };

                table.OnInternalDelete += row =>
                {
                    var key = GetKey(row);
                    var keyCache = cache[key];
                    keyCache.Remove(row);
                    if (keyCache.Count == 0)
                    {
                        cache.Remove(key);
                    }
                };
            }

            public IEnumerable<Row> Filter(Column value) =>
                cache.TryGetValue(value, out var rows) ? rows : Enumerable.Empty<Row>();
        }

        protected abstract string RemoteTableName { get; }
        string IRemoteTableHandle.RemoteTableName => RemoteTableName;

        public RemoteTableHandle(IDbConnection conn) : base(conn) { }

        // This method needs to be overridden by autogen.
        protected virtual object? GetPrimaryKey(Row row) => null;

        // These events are used by indices to add/remove rows to their dictionaries.
        // TODO: figure out if they can be merged into regular OnInsert / OnDelete.
        // I didn't do that because that delays the index updates until after the row is processed.
        // In theory, that shouldn't be the issue, but I didn't want to break it right before leaving :)
        //          - Ingvar
        private event Action<Row>? OnInternalInsert;
        private event Action<Row>? OnInternalDelete;

        // These are implementations of the type-erased interface.
        object? IRemoteTableHandle.GetPrimaryKey(IStructuralReadWrite row) => GetPrimaryKey((Row)row);

        // These are provided by RemoteTableHandle.
        Type IRemoteTableHandle.ClientTableType => typeof(Row);

        // THE DATA IN THE TABLE.
        // The keys of this map are:
        // - Primary keys, if we have them.
        // - Byte arrays, if we don't.
        // But really, the keys are whatever SpacetimeDBClient chooses to give us.
        //
        // We store the BSATN encodings of objects next to their runtime representation.
        // This is memory-inefficient, but allows us to quickly compare objects when seeing if an update is a "real"
        // update or just a multiplicity change.
        private readonly MultiDictionary<object, DbValue> Entries = new(GenericEqualityComparer.Instance, DbValueComparer.Instance);

        // The function to use for decoding a type value.
        IStructuralReadWrite IRemoteTableHandle.DecodeValue(byte[] bytes) => BSATNHelpers.Decode<Row>(bytes);

        public delegate void RowEventHandler(EventContext context, Row row);
        public event RowEventHandler? OnInsert;
        public event RowEventHandler? OnDelete;
        public event RowEventHandler? OnBeforeDelete;

        public delegate void UpdateEventHandler(EventContext context, Row oldRow, Row newRow);
        public event UpdateEventHandler? OnUpdate;

        public int Count => (int)Entries.CountDistinct;

        public IEnumerable<Row> Iter() => Entries.Entries.Select(entry => (Row)entry.Value.value);

        public Task<Row[]> RemoteQuery(string query) =>
            conn.RemoteQuery<Row>($"SELECT {RemoteTableName}.* FROM {RemoteTableName} {query}");

        void InvokeInsert(IEventContext context, IStructuralReadWrite row)
        {
            try
            {
                OnInsert?.Invoke((EventContext)context, (Row)row);
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
                OnDelete?.Invoke((EventContext)context, (Row)row);
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
                OnBeforeDelete?.Invoke((EventContext)context, (Row)row);
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
                OnUpdate?.Invoke((EventContext)context, (Row)oldRow, (Row)newRow);
            }
            catch (Exception e)
            {
                Log.Exception(e);
            }
        }

        List<KeyValuePair<object, DbValue>> wasInserted = new();
        List<(object key, DbValue oldValue, DbValue newValue)> wasUpdated = new();
        List<KeyValuePair<object, DbValue>> wasRemoved = new();

        void IRemoteTableHandle.PreApply(IEventContext context, MultiDictionaryDelta<object, DbValue> multiDictionaryDelta)
        {
            Debug.Assert(wasInserted.Count == 0 && wasUpdated.Count == 0 && wasRemoved.Count == 0, "Call Apply and PostApply before calling PreApply again");

            foreach (var (_, value) in Entries.WillRemove(multiDictionaryDelta))
            {
                InvokeBeforeDelete(context, value.value);
            }
        }

        void IRemoteTableHandle.Apply(IEventContext context, MultiDictionaryDelta<object, DbValue> multiDictionaryDelta)
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

            // Update indices.
            // This is a local operation -- it only looks at our indices and doesn't invoke user code.
            // So we don't need to wait for other tables to be updated to do it.
            // (And we need to do it before any PostApply is called.)
            foreach (var (_, value) in wasInserted)
            {
                if (value.value is Row newRow)
                {
                    OnInternalInsert?.Invoke(newRow);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {value.value.GetType().Name}");
                }
            }
            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                if (oldValue.value is Row oldRow)
                {
                    OnInternalDelete?.Invoke((Row)oldValue.value);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {oldValue.value.GetType().Name}");
                }


                if (newValue.value is Row newRow)
                {
                    OnInternalInsert?.Invoke(newRow);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {newValue.value.GetType().Name}");
                }
            }

            foreach (var (_, value) in wasRemoved)
            {
                if (value.value is Row oldRow)
                {
                    OnInternalDelete?.Invoke(oldRow);
                }
            }
        }

        void IRemoteTableHandle.PostApply(IEventContext context)
        {
            foreach (var (_, value) in wasInserted)
            {
                InvokeInsert(context, value.value);
            }
            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                InvokeUpdate(context, oldValue.value, newValue.value);
            }
            foreach (var (_, value) in wasRemoved)
            {
                InvokeDelete(context, value.value);
            }
            wasInserted.Clear();
            wasUpdated.Clear();
            wasRemoved.Clear();

        }
    }

    /// <summary>
    /// EqualityComparer used to compare primary keys.
    /// 
    /// If the primary keys are byte arrays (i.e. if the table has no primary key), uses Internal.ByteArrayComparer.
    /// Otherwise, falls back to .Equals().
    /// 
    /// TODO: we should test that this works for all of our supported primary key types.
    /// </summary>
    internal readonly struct GenericEqualityComparer : IEqualityComparer<object>
    {
        public static GenericEqualityComparer Instance = new();

        public new bool Equals(object x, object y)
        {
            if (x is byte[] x_ && y is byte[] y_)
            {
                return Internal.ByteArrayComparer.Instance.Equals(x_, y_);
            }
            return x.Equals(y); // MAKE SURE to use .Equals and not ==... that was a bug.
        }

        public int GetHashCode(object obj)
        {
            if (obj is byte[] obj_)
            {
                return Internal.ByteArrayComparer.Instance.GetHashCode(obj_);
            }
            return obj.GetHashCode();
        }

    }
}
