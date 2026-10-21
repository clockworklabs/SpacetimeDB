#if NET10_0_OR_GREATER
namespace SpacetimeDB.Internal;

// Ownership only. Database operations are added when generated handles are routed.
internal interface IDatabaseBackend : IDisposable
{
    NamespaceRegistry Namespaces { get; }
    Identity DatabaseIdentity { get; }
    void CheckActive();
    IDatabaseTransaction BeginTransaction();
}

internal interface IDatabaseTransaction : IDisposable
{
    IDatabaseBackend Backend { get; }
    void CheckActive();
    void Commit();
    void Rollback();
}

internal sealed class DatabaseAccess
{
    internal IDatabaseBackend Backend { get; }
    internal IDatabaseTransaction? Transaction { get; }

    internal DatabaseAccess(IDatabaseBackend backend)
    {
        backend.CheckActive();
        Backend = backend;
    }

    internal DatabaseAccess(IDatabaseTransaction transaction)
    {
        transaction.CheckActive();
        Backend = transaction.Backend;
        Transaction = transaction;
    }

    internal void CheckActive()
    {
        if (Transaction is { } transaction)
            transaction.CheckActive();
        else
            Backend.CheckActive();
    }
}
#endif
