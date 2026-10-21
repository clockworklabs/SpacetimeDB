namespace SpacetimeDB.Testing;

using SpacetimeDB.Internal;

internal sealed class DatabaseBackend : IDatabaseBackend
{
    private readonly NativeDatastore datastore;

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
        lock (datastore.Sync)
        {
            CheckActive();
            return new Transaction(this, datastore.Begin());
        }
    }

    public void Dispose() => datastore.Dispose();

    private sealed class Transaction(DatabaseBackend backend, NativeTransaction transaction)
        : IDatabaseTransaction
    {
        private bool finished;

        public IDatabaseBackend Backend => backend;

        public void CheckActive()
        {
            lock (backend.datastore.Sync)
            {
                backend.CheckActive();
                ObjectDisposedException.ThrowIf(transaction.IsClosed, this);
                if (finished)
                    throw new InvalidOperationException("The transaction has finished.");
            }
        }

        public void Commit()
        {
            lock (backend.datastore.Sync)
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
        }

        public void Rollback()
        {
            lock (backend.datastore.Sync)
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
        }

        public void Dispose() => transaction.Dispose();
    }
}
