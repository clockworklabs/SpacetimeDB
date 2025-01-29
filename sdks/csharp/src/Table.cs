using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

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

    internal interface IDbOps
    {
        void OnMessageProcessCompleteUpdate(IEventContext eventContext);
        HashSet<byte[]> SubscriptionInserts { get; }
        void CalculateStateDiff();
    }

    public interface IRemoteTableHandle
    {
        // These methods need to be overridden by autogen.
        object? GetPrimaryKey(IDatabaseRow row);
        void InternalInvokeValueInserted(IDatabaseRow row);
        void InternalInvokeValueDeleted(IDatabaseRow row);

        // These are provided by RemoteTableHandle.
        internal Type ClientTableType { get; }

        internal void Initialize(string name, IDbConnection conn);

        internal IDbOps PreProcessUnsubscribeApplied(IEnumerable<QueryUpdate> updates);
        internal IDbOps PreProcessTableUpdate(IEnumerable<QueryUpdate> updates);
        internal IDbOps PreProcessInsertOnlyTable(IEnumerable<QueryUpdate> updates);
    }

    public abstract class RemoteTableHandle<EventContext, Row> : IRemoteTableHandle
        where EventContext : class, IEventContext
        where Row : IDatabaseRow, new()
    {
        readonly struct DbValue
        {
            public readonly Row value;
            public readonly byte[] bytes;

            public DbValue(Row value, byte[] bytes)
            {
                this.value = value;
                this.bytes = bytes;
            }
        }

        struct DbOp
        {
            public DbValue? delete;
            public DbValue? insert;
        }

        class DbOps : List<DbOp>, IDbOps
        {
            public RemoteTableHandle<EventContext, Row> table;

            public DbOps(RemoteTableHandle<EventContext, Row> table)
            {
                this.table = table;
            }

            public HashSet<byte[]> SubscriptionInserts { get; } = new(Internal.ByteArrayComparer.Instance);

            void IDbOps.CalculateStateDiff()
            {
                AddRange(
                    table.Entries.Where(kv => !SubscriptionInserts.Contains(kv.Key))
                    .Select(kv => new DbOp
                    {
                        // This is a row that we had before, but we do not have it now.
                        // This must have been a delete.
                        delete = new(kv.Value, kv.Key),
                    })
                );
                // We won't need this anymore.
                SubscriptionInserts.Clear();
            }

            void IDbOps.OnMessageProcessCompleteUpdate(IEventContext eventContext)
            {
                foreach (var op in this)
                {
                    if (op is { delete: { value: var oldValue }, insert: null })
                    {
                        try
                        {
                            table.OnBeforeDelete?.Invoke((EventContext)eventContext, oldValue);
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }
                    }
                }

                for (var i = 0; i < Count; i++)
                {
                    var op = this[i];

                    if (op.delete is { } delete)
                    {
                        if (table.Entries.Remove(delete.bytes))
                        {
                            table.InternalInvokeValueDeleted(delete.value);
                        }
                        else
                        {
                            Log.Warn("Deleting value that we don't have (no cached value available)");
                            op.delete = null;
                            this[i] = op;
                        }
                    }

                    if (op.insert is { } insert)
                    {
                        if (table.Entries.TryAdd(insert.bytes, insert.value))
                        {
                            table.InternalInvokeValueInserted(insert.value);
                        }
                        else
                        {
                            op.insert = null;
                            this[i] = op;
                        }
                    }
                }

                var context = (EventContext)eventContext;
                foreach (var op in this)
                {
                    try
                    {
                        switch (op)
                        {
                            case { insert: { value: var newValue }, delete: { value: var oldValue } }:
                                table.OnUpdate?.Invoke(context, oldValue, newValue);
                                break;

                            case { insert: { value: var newValue } }:
                                table.OnInsert?.Invoke(context, newValue);
                                break;

                            case { delete: { value: var oldValue } }:
                                table.OnDelete?.Invoke(context, oldValue);
                                break;
                        }
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                }
            }
        }

        private DbValue Decode(byte[] bin, out object? primaryKey)
        {
            var obj = BSATNHelpers.Decode<Row>(bin);
            primaryKey = GetPrimaryKey(obj);
            return new(obj, bin);
        }

        IDbOps IRemoteTableHandle.PreProcessTableUpdate(IEnumerable<QueryUpdate> updates)
        {
            var dbOps = new DbOps(this);
            var primaryKeyChanges = new Dictionary<object?, DbOp>();

            foreach (var qu in updates)
            {
                foreach (var row in qu.Inserts)
                {
                    var op = new DbOp { insert = Decode(row, out var pk) };
                    if (pk != null)
                    {
                        // Compound key that we use for lookup.
                        // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                        var key = (this, pk);

                        if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                        {
                            if (oldOp.insert is not null)
                            {
                                Log.Warn($"Update with the same primary key was applied multiple times! tableName={name}");
                                // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                // SpacetimeDB side.
                                continue;
                            }

                            op.delete = oldOp.delete;
                        }
                        primaryKeyChanges[key] = op;
                    }
                    else
                    {
                        dbOps.Add(op);
                    }
                }

                foreach (var row in qu.Deletes)
                {
                    var op = new DbOp { delete = Decode(row, out var pk) };
                    if (pk != null)
                    {
                        // Compound key that we use for lookup.
                        // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                        var key = (this, pk);

                        if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                        {
                            if (oldOp.delete is not null)
                            {
                                Log.Warn($"Update with the same primary key was applied multiple times! tableName={name}");
                                // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                // SpacetimeDB side.
                                continue;
                            }

                            op.insert = oldOp.insert;
                        }
                        primaryKeyChanges[key] = op;
                    }
                    else
                    {
                        dbOps.Add(op);
                    }
                }
            }

            return dbOps;
        }

        /// <summary>
        /// TODO: the dictionary is here for backwards compatibility and can be removed
        /// once we get rid of legacy subscriptions.
        /// </summary>
        IDbOps IRemoteTableHandle.PreProcessUnsubscribeApplied(IEnumerable<QueryUpdate> updates)
        {
            var dbOps = new DbOps(this);

            // First apply all of the state
            foreach (var qu in updates)
            {
                if (qu.Inserts.RowsData.Count > 0)
                {
                    Log.Warn("Non-insert during an UnsubscribeApplied!");
                }
                foreach (var bin in qu.Deletes)
                {
                    var obj = BSATNHelpers.Decode<Row>(bin);
                    var op = new DbOp
                    {
                        delete = new(obj, bin),
                    };
                    dbOps.Add(op);
                }
            }

            return dbOps;
        }

        IDbOps IRemoteTableHandle.PreProcessInsertOnlyTable(IEnumerable<QueryUpdate> updates)
        {
            var dbOps = new DbOps(this);

            foreach (var qu in updates)
            {
                if (qu.Deletes.RowsData.Count > 0)
                {
                    Log.Warn("Non-insert during an insert-only server message!");
                }
                foreach (var bin in qu.Inserts)
                {
                    if (!dbOps.SubscriptionInserts.Add(bin))
                    {
                        // Ignore duplicate inserts in the same subscription update.
                        continue;
                    }
                    var obj = BSATNHelpers.Decode<Row>(bin);
                    var op = new DbOp
                    {
                        insert = new(obj, bin),
                    };
                    dbOps.Add(op);
                }
            }

            return dbOps;
        }

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
            conn!.RemoteQuery<Row>($"SELECT {name!}.* FROM {name!} {query}");
    }
}
