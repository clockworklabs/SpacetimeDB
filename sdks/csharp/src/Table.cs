using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
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

        /// <summary>
        /// Creates and returns a parsed table update for the current table.
        /// Note: The returned <see cref="IParsedTableUpdate"/> is type-erased because <see cref="IRemoteTableHandle"/> is also type-erased.
        /// To use the parsed update, you must downcast it to its concrete type.
        /// </summary>
        /// <returns>An <see cref="IParsedTableUpdate"/> representing the parsed update.</returns>
        internal IParsedTableUpdate MakeParsedTableUpdate();

        /// <summary>
        /// Parses an insert-only table update and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing insert operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        internal void ParseInsertOnly(TableUpdate update, ParsedDatabaseUpdate dbOps);

        /// <summary>
        /// Parses a delete-only table update and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing delete operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        internal void ParseDeleteOnly(TableUpdate update, ParsedDatabaseUpdate dbOps);

        /// <summary>
        /// Parses a general table update (insert, delete) and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        internal void Parse(TableUpdate update, ParsedDatabaseUpdate dbOps);

        /// <summary>
        /// Start applying a delta to the table.
        /// This is called for all tables before any updates are actually applied, allowing OnBeforeDelete to be invoked correctly.
        /// </summary>
        /// <param name="context"></param>
        /// <param name="parsedTableUpdate"></param>
        internal void PreApply(IEventContext context, IParsedTableUpdate parsedTableUpdate);

        /// <summary>
        /// Apply a delta to the table.
        /// Should not invoke any user callbacks, since not all tables have been updated yet.
        /// Should fix up indices, to be ready for PostApply.
        /// </summary>
        /// <param name="context"></param>
        /// <param name="parsedTableUpdate"></param>
        internal void Apply(IEventContext context, IParsedTableUpdate parsedTableUpdate);

        /// <summary>
        /// Finish applying a delta to a table.
        /// This is when row callbacks (besides OnBeforeDelete) actually happen.
        /// </summary>
        internal void PostApply(IEventContext context);
    }

    interface IParsedTableUpdate
    {
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

        public abstract class NullableRefUniqueIndexBase<Column>
            where Column : class, IEquatable<Column>
        {
            protected abstract Column? GetKey(Row row);

            private readonly Dictionary<Column, Row> cache = new();

            public NullableRefUniqueIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row =>
                {
                    var key = GetKey(row);
                    if (key == null)
                    {
                        return;
                    }
                    cache.Add(key, row);
                };
                table.OnInternalDelete += row =>
                {
                    var key = GetKey(row);
                    if (key == null)
                    {
                        return;
                    }
                    cache.Remove(key);
                };
            }

            public Row? Find(Column value) => cache.TryGetValue(value, out var row) ? row : null;
        }

        public abstract class NullableRefBTreeIndexBase<Column>
            where Column : class, IEquatable<Column>, IComparable<Column>
        {
            protected abstract Column? GetKey(Row row);

            private readonly Dictionary<Column, HashSet<Row>> cache = new();

            public NullableRefBTreeIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row =>
                {
                    var key = GetKey(row);
                    if (key == null)
                    {
                        return;
                    }
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
                    if (key == null)
                    {
                        return;
                    }
                    if (!cache.TryGetValue(key, out var keyCache))
                    {
                        return;
                    }
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

        public abstract class NullableValueUniqueIndexBase<Column>
            where Column : struct, IEquatable<Column>
        {
            protected abstract Column? GetKey(Row row);

            private readonly Dictionary<Column, Row> cache = new();

            public NullableValueUniqueIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row =>
                {
                    var key = GetKey(row);
                    if (!key.HasValue)
                    {
                        return;
                    }
                    cache.Add(key.Value, row);
                };
                table.OnInternalDelete += row =>
                {
                    var key = GetKey(row);
                    if (!key.HasValue)
                    {
                        return;
                    }
                    cache.Remove(key.Value);
                };
            }

            public Row? Find(Column value) => cache.TryGetValue(value, out var row) ? row : null;
        }

        public abstract class NullableValueBTreeIndexBase<Column>
            where Column : struct, IEquatable<Column>, IComparable<Column>
        {
            protected abstract Column? GetKey(Row row);

            private readonly Dictionary<Column, HashSet<Row>> cache = new();

            public NullableValueBTreeIndexBase(RemoteTableHandle<EventContext, Row> table)
            {
                table.OnInternalInsert += row =>
                {
                    var key = GetKey(row);
                    if (!key.HasValue)
                    {
                        return;
                    }
                    if (!cache.TryGetValue(key.Value, out var rows))
                    {
                        rows = new();
                        cache.Add(key.Value, rows);
                    }
                    rows.Add(row);
                };

                table.OnInternalDelete += row =>
                {
                    var key = GetKey(row);
                    if (!key.HasValue)
                    {
                        return;
                    }
                    if (!cache.TryGetValue(key.Value, out var keyCache))
                    {
                        return;
                    }
                    keyCache.Remove(row);
                    if (keyCache.Count == 0)
                    {
                        cache.Remove(key.Value);
                    }
                };
            }

            public IEnumerable<Row> Filter(Column value) =>
                cache.TryGetValue(value, out var rows) ? rows : Enumerable.Empty<Row>();
        }

        /// <summary>
        /// Represents a parsed update to a table, storing the changes as a multi-dictionary delta
        /// mapping primary keys to their corresponding row updates.
        /// </summary>
        internal class ParsedTableUpdate : IParsedTableUpdate
        {
            /// <summary>
            /// Stores the set of changes for the table, mapping primary keys to updated rows.
            /// </summary>
            internal MultiDictionaryDelta<object, Row> Delta = new(EqualityComparer<object>.Default, EqualityComparer<Row>.Default);
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
        private AbstractEventHandler<Row> OnInternalInsertHandler { get; } = new();
        private event Action<Row> OnInternalInsert
        {
            add => OnInternalInsertHandler.AddListener(value);
            remove => OnInternalInsertHandler.RemoveListener(value);
        }
        private AbstractEventHandler<Row> OnInternalDeleteHandler { get; } = new();
        private event Action<Row> OnInternalDelete
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
        private readonly MultiDictionary<object, Row> Entries = new(EqualityComparer<object>.Default, EqualityComparer<Row>.Default);

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
        Row DecodeValue(BinaryReader reader) => Serializer.Read(reader);

        /// <summary>
        /// Decode a row for a table, producing a primary key.
        /// If the table has a specific column marked `#[primary_key]`, use that.
        /// If not, the BSATN for the entire row is used instead.
        /// </summary>
        /// <param name="table"></param>
        /// <param name="reader"></param>
        /// <param name="primaryKey"></param>
        /// <returns></returns>
        public Row Decode(BinaryReader reader, out object primaryKey)
        {
            var obj = DecodeValue(reader);

            // TODO(1.1): we should exhaustively check that GenericEqualityComparer works
            // for all types that are allowed to be primary keys.
            var primaryKey_ = GetPrimaryKey(obj);
            primaryKey_ ??= obj;
            primaryKey = primaryKey_;

            return obj;
        }

        /// <summary>
        /// Creates and returns a parsed table update for the current table.
        /// </summary>
        /// <returns>An <see cref="IParsedTableUpdate"/> representing the parsed update.</returns>
        IParsedTableUpdate IRemoteTableHandle.MakeParsedTableUpdate()
        {
            return new ParsedTableUpdate();
        }

        /// <summary>
        /// Parses an insert-only table update and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing insert operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        void IRemoteTableHandle.ParseInsertOnly(TableUpdate update, ParsedDatabaseUpdate dbOps)
        {
            var delta = (ParsedTableUpdate)dbOps.UpdateForTable(this);

            foreach (var cqu in update.Updates)
            {
                var qu = CompressionHelpers.DecompressDecodeQueryUpdate(cqu);
                if (qu.Deletes.RowsData.Count > 0)
                {
                    Log.Warn("Non-insert during an insert-only server message!");
                }
                var (insertReader, insertRowCount) = CompressionHelpers.ParseRowList(qu.Inserts);
                for (var i = 0; i < insertRowCount; i++)
                {
                    var obj = Decode(insertReader, out var pk);
                    delta.Delta.Add(pk, obj);
                }
            }
        }

        /// <summary>
        /// Parses a delete-only table update and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing delete operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        void IRemoteTableHandle.ParseDeleteOnly(TableUpdate update, ParsedDatabaseUpdate dbOps)
        {
            var delta = (ParsedTableUpdate)dbOps.UpdateForTable(this);
            foreach (var cqu in update.Updates)
            {
                var qu = CompressionHelpers.DecompressDecodeQueryUpdate(cqu);
                if (qu.Inserts.RowsData.Count > 0)
                {
                    Log.Warn("Non-delete during a delete-only operation!");
                }

                var (deleteReader, deleteRowCount) = CompressionHelpers.ParseRowList(qu.Deletes);
                for (var i = 0; i < deleteRowCount; i++)
                {
                    var obj = Decode(deleteReader, out var pk);
                    delta.Delta.Remove(pk, obj);
                }
            }
        }

        /// <summary>
        /// Parses a general table update (insert, update, delete) and applies the results to the specified parsed database update.
        /// </summary>
        /// <param name="update">The table update containing operations.</param>
        /// <param name="dbOps">The parsed database update to apply changes to.</param>
        void IRemoteTableHandle.Parse(TableUpdate update, ParsedDatabaseUpdate dbOps)
        {
            var delta = (ParsedTableUpdate)dbOps.UpdateForTable(this);
            foreach (var cqu in update.Updates)
            {
                var qu = CompressionHelpers.DecompressDecodeQueryUpdate(cqu);

                // Because we are accumulating into a MultiDictionaryDelta that will be applied all-at-once
                // to the table, it doesn't matter that we call Add before Remove here.

                var (insertReader, insertRowCount) = CompressionHelpers.ParseRowList(qu.Inserts);
                for (var i = 0; i < insertRowCount; i++)
                {
                    var obj = Decode(insertReader, out var pk);
                    delta.Delta.Add(pk, obj);
                }

                var (deleteReader, deleteRowCount) = CompressionHelpers.ParseRowList(qu.Deletes);
                for (var i = 0; i < deleteRowCount; i++)
                {
                    var obj = Decode(deleteReader, out var pk);
                    delta.Delta.Remove(pk, obj);
                }
            }

        }

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

        public IEnumerable<Row> Iter() => Entries.Entries.Select(entry => (Row)entry.Value);

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

        List<KeyValuePair<object, Row>> wasInserted = new();
        List<(object key, Row oldValue, Row newValue)> wasUpdated = new();
        List<KeyValuePair<object, Row>> wasRemoved = new();

        /// <summary>
        /// Invoked before applying the parsed table update (delta) to this table.
        /// This is called for all tables before any updates are applied, allowing OnBeforeDelete callbacks to be triggered for rows that will be removed.
        /// Calling the OnBeforeDelete callbacks allows the user to read the old values of the rows that will be removed, before they are actually removed.
        /// Should be called before Apply and PostApply.
        /// </summary>
        void IRemoteTableHandle.PreApply(IEventContext context, IParsedTableUpdate parsedTableUpdate)
        {
            Debug.Assert(wasInserted.Count == 0 && wasUpdated.Count == 0 && wasRemoved.Count == 0, "Call Apply and PostApply before calling PreApply again");
            var delta = (ParsedTableUpdate)parsedTableUpdate;
            foreach (var (_, value) in Entries.WillRemove(delta.Delta))
            {
                InvokeBeforeDelete(context, value);
            }
        }

        /// <summary>
        /// Applies the parsed table update (delta) to this table.
        /// This updates the internal data structures and indices, but does not invoke user callbacks.
        /// Should be called before PostApply, after PreApply.
        /// </summary>
        void IRemoteTableHandle.Apply(IEventContext context, IParsedTableUpdate parsedTableUpdate)
        {
            try
            {
                var delta = (ParsedTableUpdate)parsedTableUpdate;
                Entries.Apply(delta.Delta, wasInserted, wasUpdated, wasRemoved);
            }
            catch (Exception e)
            {
                var deltaString = parsedTableUpdate.ToString();
                deltaString = deltaString[..Math.Min(deltaString.Length, 10_000)];
                var entriesString = Entries.ToString();
                entriesString = entriesString[..Math.Min(entriesString.Length, 10_000)];
                throw new Exception($"While table `{RemoteTableName}` was applying:\n{deltaString} \nto:\n{entriesString}", e);
            }

            // Update indices.
            // This is a local operation -- it only looks at our indices and doesn't invoke user code.
            // So we don't need to wait for other tables to be updated to do it.
            // (And we need to do it before any PostApply is called.)
            // Reminder: We need to loop through the removed entries to delete them prior to inserting the new entries,
            // in order to avoid keys an error with the same key already added.
            foreach (var (_, value) in wasRemoved)
            {
                if (value is Row oldRow)
                {
                    OnInternalDeleteHandler.Invoke(oldRow);
                }
            }
            foreach (var (_, value) in wasInserted)
            {
                if (value is Row newRow)
                {
                    OnInternalInsertHandler.Invoke(newRow);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {value.GetType().Name}");
                }
            }
            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                if (oldValue is Row oldRow)
                {
                    OnInternalDeleteHandler.Invoke(oldRow);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {oldValue.GetType().Name}");
                }


                if (newValue is Row newRow)
                {
                    OnInternalInsertHandler.Invoke(newRow);
                }
                else
                {
                    throw new Exception($"Invalid row type for table {RemoteTableName}: {newValue.GetType().Name}");
                }
            }
        }

        /// <summary>
        /// Invoked after applying the parsed table update (delta) to this table.
        /// This is when user callbacks (such as OnInsert, OnUpdate, and OnDelete) are actually triggered for the affected rows.
        /// All <see cref="IRemoteTableHandle.Apply"/> operations should be complete before calling PostApply,
        /// so that data structures across all tables are fully updated before invoking user callbacks.
        /// Should be called after PreApply and Apply.
        /// </summary>
        void IRemoteTableHandle.PostApply(IEventContext context)
        {
            foreach (var (_, value) in wasInserted)
            {
                InvokeInsert(context, value);
            }
            foreach (var (_, oldValue, newValue) in wasUpdated)
            {
                InvokeUpdate(context, oldValue, newValue);
            }
            foreach (var (_, value) in wasRemoved)
            {
                InvokeDelete(context, value);
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
#nullable disable