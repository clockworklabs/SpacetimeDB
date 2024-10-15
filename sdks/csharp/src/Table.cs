using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

using SpacetimeDB.BSATN;

namespace SpacetimeDB
{
    public interface IDatabaseRow : IStructuralReadWrite { }

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
        // These methods need to be overridden by autogen.
        object? GetPrimaryKey(IDatabaseRow row);
        void InternalInvokeValueInserted(IDatabaseRow row);
        void InternalInvokeValueDeleted(IDatabaseRow row);

        // These are provided by RemoteTableHandle.
        internal Type ClientTableType { get; }
        internal IEnumerable<KeyValuePair<byte[], IDatabaseRow>> IterEntries();
        internal bool InsertEntry(byte[] rowBytes, IDatabaseRow value);
        internal bool DeleteEntry(byte[] rowBytes);
        internal IDatabaseRow DecodeValue(byte[] bytes);

        internal void InvokeInsert(IEventContext context, IDatabaseRow row);
        internal void InvokeDelete(IEventContext context, IDatabaseRow row);
        internal void InvokeBeforeDelete(IEventContext context, IDatabaseRow row);
        internal void InvokeUpdate(IEventContext context, IDatabaseRow oldRow, IDatabaseRow newRow);

        internal void Initialize(string name, IDbConnection conn);
    }

    public abstract class RemoteTableHandle<EventContext, Row> : IRemoteTableHandle
        where EventContext : class, IEventContext
        where Row : IDatabaseRow, new()
    {
        string? name;
        IDbConnection? conn;

        void IRemoteTableHandle.Initialize(string name, IDbConnection conn)
        {
            this.name = name;
            this.conn = conn;
        }

        // These methods need to be overridden by autogen.
        public virtual object? GetPrimaryKey(IDatabaseRow row) => null;
        public virtual void InternalInvokeValueInserted(IDatabaseRow row) { }
        public virtual void InternalInvokeValueDeleted(IDatabaseRow row) { }

        // These are provided by RemoteTableHandle.
        Type IRemoteTableHandle.ClientTableType => typeof(Row);

        private readonly Dictionary<byte[], Row> Entries = new(Internal.ByteArrayComparer.Instance);

        IEnumerable<KeyValuePair<byte[], IDatabaseRow>> IRemoteTableHandle.IterEntries() =>
            Entries.Select(kv => new KeyValuePair<byte[], IDatabaseRow>(kv.Key, kv.Value));

        /// <summary>
        /// Inserts the value into the table. There can be no existing value with the provided BSATN bytes.
        /// </summary>
        /// <param name="rowBytes">The BSATN encoded bytes of the row to retrieve.</param>
        /// <param name="value">The parsed row encoded by the <paramref>rowBytes</paramref>.</param>
        /// <returns>True if the row was inserted, false if the row wasn't inserted because it was a duplicate.</returns>
        bool IRemoteTableHandle.InsertEntry(byte[] rowBytes, IDatabaseRow value) => Entries.TryAdd(rowBytes, (Row)value);

        /// <summary>
        /// Deletes a value from the table.
        /// </summary>
        /// <param name="rowBytes">The BSATN encoded bytes of the row to remove.</param>
        /// <returns>True if and only if the value was previously resident and has been deleted.</returns>
        bool IRemoteTableHandle.DeleteEntry(byte[] rowBytes)
        {
            if (Entries.Remove(rowBytes))
            {
                return true;
            }

            Log.Warn("Deleting value that we don't have (no cached value available)");
            return false;
        }

        // The function to use for decoding a type value.
        IDatabaseRow IRemoteTableHandle.DecodeValue(byte[] bytes) => BSATNHelpers.Decode<Row>(bytes);

        public event Action<EventContext, Row>? OnInsert;
        public event Action<EventContext, Row>? OnDelete;
        public event Action<EventContext, Row>? OnBeforeDelete;
        public event Action<EventContext, Row, Row>? OnUpdate;

        public int Count => Entries.Count;

        public IEnumerable<Row> Iter() => Entries.Values;

        protected IEnumerable<Row> Query(Func<Row, bool> filter) => Iter().Where(filter);

        public Task<Row[]> RemoteQuery(string query) =>
            conn!.RemoteQuery<Row>($"SELECT * FROM {name!} {query}");

        void IRemoteTableHandle.InvokeInsert(IEventContext context, IDatabaseRow row) =>
            OnInsert?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeDelete(IEventContext context, IDatabaseRow row) =>
            OnDelete?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeBeforeDelete(IEventContext context, IDatabaseRow row) =>
            OnBeforeDelete?.Invoke((EventContext)context, (Row)row);

        void IRemoteTableHandle.InvokeUpdate(IEventContext context, IDatabaseRow oldRow, IDatabaseRow newRow) =>
            OnUpdate?.Invoke((EventContext)context, (Row)oldRow, (Row)newRow);
    }
}
