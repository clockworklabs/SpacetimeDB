#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

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
    ulong Count(string assemblyIdentity, string table);
    byte[] Insert(string assemblyIdentity, string table, byte[] row);
    byte[] Scan(string assemblyIdentity, string table);
}

internal sealed class DatabaseAccess
{
    internal static DatabaseAccess Require(DatabaseAccess? access) =>
        access
        ?? throw new InvalidOperationException(
            "Managed database access requires a bound testing backend."
        );

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
