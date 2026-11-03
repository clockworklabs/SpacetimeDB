namespace SpacetimeDB.Testing;

using SpacetimeDB.Internal;

// One execution flow owns each fixture. Native handles synchronize calls and
// destruction separately because finalizers can run on another thread.
internal sealed class DatabaseBackend : IDatabaseBackend
{
    private readonly NativeDatastore datastore;
    private readonly Dictionary<(string Assembly, string Table), uint> tableIds = [];
    private readonly Dictionary<(string Assembly, string Index), uint> indexIds = [];

    public NamespaceRegistry Namespaces { get; }
    public Identity DatabaseIdentity => default;

    internal DatabaseBackend(ReadOnlySpan<byte> schema, NamespaceRegistry namespaces)
    {
        Namespaces = namespaces;
        datastore = NativeDatastore.Create(schema);
    }

    public void CheckActive() => ObjectDisposedException.ThrowIf(datastore.IsClosed, this);

    public IDatabaseTransaction BeginTransaction()
    {
        CheckActive();
        return new Transaction(this, datastore.Begin());
    }

    public void Dispose() => datastore.Dispose();

    private sealed class Transaction(DatabaseBackend backend, NativeTransaction transaction)
        : IDatabaseTransaction
    {
        private bool finished;

        public IDatabaseBackend Backend => backend;

        private uint TableId(string assemblyIdentity, string table)
        {
            var key = (assemblyIdentity, table);
            if (!backend.tableIds.TryGetValue(key, out var id))
            {
                id = transaction.TableId(backend.Namespaces.Resolve(assemblyIdentity, table));
                backend.tableIds.Add(key, id);
            }
            return id;
        }

        private uint IndexId(string assemblyIdentity, string index)
        {
            var key = (assemblyIdentity, index);
            if (!backend.indexIds.TryGetValue(key, out var id))
            {
                id = transaction.IndexId(backend.Namespaces.Resolve(assemblyIdentity, index));
                backend.indexIds.Add(key, id);
            }
            return id;
        }

        public byte[] IndexScanPoint(string assemblyIdentity, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexScanPoint(IndexId(assemblyIdentity, index), point);
        }

        public bool IndexDeletePoint(string assemblyIdentity, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexDeletePoint(IndexId(assemblyIdentity, index), point);
        }

        public byte[] IndexScanRange(
            string assemblyIdentity,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexScanRange(
                IndexId(assemblyIdentity, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public uint IndexDeleteRange(
            string assemblyIdentity,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexDeleteRange(
                IndexId(assemblyIdentity, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public byte[] Update(string assemblyIdentity, string table, string index, byte[] row)
        {
            CheckActive();
            return transaction.Update(
                TableId(assemblyIdentity, table),
                IndexId(assemblyIdentity, index),
                row
            );
        }

        public ulong Count(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Count(TableId(assemblyIdentity, table));
        }

        public byte[] Insert(string assemblyIdentity, string table, byte[] row)
        {
            CheckActive();
            return transaction.Insert(TableId(assemblyIdentity, table), row);
        }

        public bool Delete(string assemblyIdentity, string table, byte[] row)
        {
            CheckActive();
            return transaction.Delete(TableId(assemblyIdentity, table), row);
        }

        public ulong Clear(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Clear(TableId(assemblyIdentity, table));
        }

        public byte[] Scan(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Scan(TableId(assemblyIdentity, table));
        }

        public void CheckActive()
        {
            backend.CheckActive();
            ObjectDisposedException.ThrowIf(transaction.IsClosed, this);
            if (finished)
                throw new InvalidOperationException("The transaction has finished.");
        }

        public void Commit()
        {
            CheckActive();
            try
            {
                transaction.Commit();
            }
            finally
            {
                finished = true;
            }
        }

        public void Rollback()
        {
            CheckActive();
            try
            {
                transaction.Rollback();
            }
            finally
            {
                finished = true;
            }
        }

        public void Dispose() => transaction.Dispose();
    }
}
