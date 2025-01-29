using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

using SpacetimeDB.BSATN;

namespace SpacetimeDB
{
    public abstract class RemoteBase<DbConnection>
    {
        protected readonly DbConnection conn;

        protected RemoteBase(DbConnection conn)
        {
            this.conn = conn;
        }
    }

    public interface IRemoteTableHandle
    {
        internal object? GetPrimaryKey(IStructuralReadWrite row);
        internal string Name { get; }

        internal Type ClientTableType { get; }
        internal IEnumerable<KeyValuePair<byte[], IStructuralReadWrite>> IterEntries();
        internal bool InsertEntry(byte[] rowBytes, IStructuralReadWrite value);
        internal bool DeleteEntry(byte[] rowBytes);
        internal IStructuralReadWrite DecodeValue(byte[] bytes);

        internal void InvokeInsert(IEventContext context, IStructuralReadWrite row);
        internal void InvokeDelete(IEventContext context, IStructuralReadWrite row);
        internal void InvokeBeforeDelete(IEventContext context, IStructuralReadWrite row);
        internal void InvokeUpdate(IEventContext context, IStructuralReadWrite oldRow, IStructuralReadWrite newRow);

        internal void Initialize(IDbConnection conn);
    }

    public abstract class RemoteTableHandle<EventContext, Row> : IRemoteTableHandle
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
            private readonly Dictionary<Column, List<Row>> cache = new();

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

        protected abstract string Name { get; }
        string IRemoteTableHandle.Name => Name;
        IDbConnection? conn;

        void IRemoteTableHandle.Initialize(IDbConnection conn) => this.conn = conn;

        // This method needs to be overridden by autogen.
        protected virtual object? GetPrimaryKey(Row row) => null;

        // These events are used by indices to add/remove rows to their dictionaries.
        // TODO: figure out if they can be merged into regular OnInsert / OnDelete.
        // I didn't do that because that delays the index updates until after the row is processed.
        // In theory, that shouldn't be the issue, but I didn't want to break it right before leaving :)
        private event Action<Row>? OnInternalInsert;
        private event Action<Row>? OnInternalDelete;

        // These are implementations of the type-erased interface.
        object? IRemoteTableHandle.GetPrimaryKey(IStructuralReadWrite row) => GetPrimaryKey((Row)row);

        // These are provided by RemoteTableHandle.
        Type IRemoteTableHandle.ClientTableType => typeof(Row);

        private readonly Dictionary<byte[], Row> Entries = new(Internal.ByteArrayComparer.Instance);

        IEnumerable<KeyValuePair<byte[], IStructuralReadWrite>> IRemoteTableHandle.IterEntries() =>
            Entries.Select(kv => new KeyValuePair<byte[], IStructuralReadWrite>(kv.Key, kv.Value));

        /// <summary>
        /// Inserts the value into the table. There can be no existing value with the provided BSATN bytes.
        /// </summary>
        /// <param name="rowBytes">The BSATN encoded bytes of the row to retrieve.</param>
        /// <param name="value">The parsed row encoded by the <paramref>rowBytes</paramref>.</param>
        /// <returns>True if the row was inserted, false if the row wasn't inserted because it was a duplicate.</returns>
        bool IRemoteTableHandle.InsertEntry(byte[] rowBytes, IStructuralReadWrite value)
        {
            var row = (Row)value;
            if (Entries.TryAdd(rowBytes, row))
            {
                OnInternalInsert?.Invoke(row);
                return true;
            }
            else
            {
                return false;
            }
        }

        /// <summary>
        /// Deletes a value from the table.
        /// </summary>
        /// <param name="rowBytes">The BSATN encoded bytes of the row to remove.</param>
        /// <returns>True if and only if the value was previously resident and has been deleted.</returns>
        bool IRemoteTableHandle.DeleteEntry(byte[] rowBytes)
        {
            if (Entries.Remove(rowBytes, out var row))
            {
                OnInternalDelete?.Invoke(row);
                return true;
            }

            Log.Warn("Deleting value that we don't have (no cached value available)");
            return false;
        }

        // The function to use for decoding a type value.
        IStructuralReadWrite IRemoteTableHandle.DecodeValue(byte[] bytes) => BSATNHelpers.Decode<Row>(bytes);

        public delegate void RowEventHandler(EventContext context, Row row);
        public event RowEventHandler? OnInsert;
        public event RowEventHandler? OnDelete;
        public event RowEventHandler? OnBeforeDelete;

        public delegate void UpdateEventHandler(EventContext context, Row oldRow, Row newRow);
        public event UpdateEventHandler? OnUpdate;

        public int Count => Entries.Count;

        public IEnumerable<Row> Iter() => Entries.Values;

        protected IEnumerable<Row> Query(Func<Row, bool> filter) => Iter().Where(filter);

        public Task<Row[]> RemoteQuery(string query) =>
            conn!.RemoteQuery<Row>($"SELECT {Name}.* FROM {Name} {query}");

        void IRemoteTableHandle.InvokeInsert(IEventContext context, IStructuralReadWrite row) =>
            OnInsert?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeDelete(IEventContext context, IStructuralReadWrite row) =>
            OnDelete?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeBeforeDelete(IEventContext context, IStructuralReadWrite row) =>
            OnBeforeDelete?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeUpdate(IEventContext context, IStructuralReadWrite oldRow, IStructuralReadWrite newRow) =>
            OnUpdate?.Invoke((EventContext)context, (Row)oldRow, (Row)newRow);
    }
}
