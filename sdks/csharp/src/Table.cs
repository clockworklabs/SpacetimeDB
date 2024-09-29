using System;
using System.Collections.Generic;
using System.Linq;

using SpacetimeDB.BSATN;

namespace SpacetimeDB
{
    public interface IDatabaseRow : IStructuralReadWrite { }

    public abstract class RemoteBase<DbConnection> {
        protected readonly DbConnection conn;

        protected RemoteBase(DbConnection conn) {
            this.conn = conn;
        }
    }

    public interface IRemoteTableHandle {
        void SetCache(ClientCache.ITableCache cache);

        object? GetPrimaryKey(IDatabaseRow row);

        void InternalInvokeValueInserted(IDatabaseRow row);
        void InternalInvokeValueDeleted(IDatabaseRow row);
        void InvokeInsert(IEventContext context, IDatabaseRow row);
        void InvokeDelete(IEventContext context, IDatabaseRow row);
        void InvokeBeforeDelete(IEventContext context, IDatabaseRow row);
        void InvokeUpdate(IEventContext context, IDatabaseRow oldRow, IDatabaseRow newRow);
    }

    public abstract class RemoteTableHandle<EventContext, Row> : IRemoteTableHandle
        where EventContext : class, IEventContext
        where Row : IDatabaseRow, new()
    {
        public void SetCache(ClientCache.ITableCache cache) => Cache = (ClientCache.TableCache<Row>)cache;

        internal ClientCache.TableCache<Row>? Cache;

        public event Action<EventContext, Row>? OnInsert;
        public event Action<EventContext, Row>? OnDelete;
        public event Action<EventContext, Row>? OnBeforeDelete;
        public event Action<EventContext, Row, Row>? OnUpdate;

        public virtual object? GetPrimaryKey(IDatabaseRow row) => null;

        public virtual void InternalInvokeValueInserted(IDatabaseRow row) { }

        public virtual void InternalInvokeValueDeleted(IDatabaseRow row) { }

        public int Count => Cache!.Entries.Count;

        public IEnumerable<Row> Iter() => Cache!.Entries.Values;

        public IEnumerable<Row> Query(Func<Row, bool> filter) => Iter().Where(filter);

        public void InvokeInsert(IEventContext context, IDatabaseRow row) =>
            OnInsert?.Invoke((EventContext)context, (Row)row);

        public void InvokeDelete(IEventContext context, IDatabaseRow row) =>
            OnDelete?.Invoke((EventContext)context, (Row)row);

        public void InvokeBeforeDelete(IEventContext context, IDatabaseRow row) =>
            OnBeforeDelete?.Invoke((EventContext)context, (Row)row);

        public void InvokeUpdate(IEventContext context, IDatabaseRow oldRow, IDatabaseRow newRow) =>
            OnUpdate?.Invoke((EventContext)context, (Row)oldRow, (Row)newRow);
    }
}