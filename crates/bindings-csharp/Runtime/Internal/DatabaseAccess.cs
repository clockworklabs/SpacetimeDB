#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

internal interface IDatabaseBackend : IDisposable
{
    NamespaceRegistry Namespaces { get; }
    Identity DatabaseIdentity { get; }
    string? GetEnv(string key);
    void CheckActive();
    IDatabaseTransaction BeginTransaction();
}

internal interface IDatabaseTransaction : IDisposable
{
    IDatabaseBackend Backend { get; }
    void CheckActive();
    void Commit();
    void Rollback();
    ulong Count(int instanceId, string table);
    byte[] Insert(int instanceId, string table, byte[] row);
    bool Delete(int instanceId, string table, byte[] row);
    ulong Clear(int instanceId, string table);
    byte[] Scan(int instanceId, string table);
    byte[] IndexScanPoint(int instanceId, string index, byte[] point);
    bool IndexDeletePoint(int instanceId, string index, byte[] point);
    byte[] IndexScanRange(
        int instanceId,
        string index,
        ushort prefixElems,
        byte[] prefix,
        byte[] start,
        byte[] end
    );
    uint IndexDeleteRange(
        int instanceId,
        string index,
        ushort prefixElems,
        byte[] prefix,
        byte[] start,
        byte[] end
    );
    byte[] Update(int instanceId, string table, string index, byte[] row);
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
