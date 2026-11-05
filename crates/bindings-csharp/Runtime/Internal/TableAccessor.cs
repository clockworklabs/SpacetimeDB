#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

/// <summary>Ownership carried by generated table and index handles.</summary>
public readonly struct TableAccessor
{
    private readonly DatabaseAccess? access;
    private readonly int contextInstance;
    private readonly string assemblyIdentity;
    private readonly string table;

    public TableAccessor(Local? db, string assemblyIdentity, string table)
        : this(db?.Access, db?.InstanceId ?? 0, assemblyIdentity, table) { }

    public TableAccessor(LocalReadOnly? db, string assemblyIdentity, string table)
        : this(db?.Access, db?.InstanceId ?? 0, assemblyIdentity, table) { }

    private TableAccessor(
        DatabaseAccess? access,
        int contextInstance,
        string assemblyIdentity,
        string table
    )
    {
        this.access = access;
        this.contextInstance = contextInstance;
        this.assemblyIdentity = assemblyIdentity;
        this.table = table;
    }

    private TResult Execute<TResult>(Func<IDatabaseTransaction, int, TResult> body)
    {
        var access = DatabaseAccess.Require(this.access);
        access.CheckActive();
        var instance = access.Backend.Namespaces.ResolveInstance(contextInstance, assemblyIdentity);
        if (access.Transaction is { } transaction)
            return body(transaction, instance);
        using var standalone = access.Backend.BeginTransaction();
        var result = body(standalone, instance);
        standalone.Commit();
        return result;
    }

    internal ulong Count()
    {
        var name = table;
        return Execute((tx, instance) => tx.Count(instance, name));
    }

    internal byte[] Insert(byte[] row)
    {
        var name = table;
        return Execute((tx, instance) => tx.Insert(instance, name, row));
    }

    internal bool Delete(byte[] row)
    {
        var name = table;
        return Execute((tx, instance) => tx.Delete(instance, name, row));
    }

    internal ulong Clear()
    {
        var name = table;
        return Execute((tx, instance) => tx.Clear(instance, name));
    }

    internal IEnumerable<T> Iter<T>()
        where T : IStructuralReadWrite, new()
    {
        var name = table;
        return Iter<T>((tx, instance) => tx.Scan(instance, name));
    }

    internal IEnumerable<T> IndexScanPoint<T>(string index, byte[] point)
        where T : IStructuralReadWrite, new()
    {
        return Iter<T>((tx, instance) => tx.IndexScanPoint(instance, index, point));
    }

    internal bool IndexDeletePoint(string index, byte[] point)
    {
        return Execute((tx, instance) => tx.IndexDeletePoint(instance, index, point));
    }

    internal IEnumerable<T> IndexScanRange<T>(
        string index,
        ushort prefixElems,
        byte[] prefix,
        byte[] start,
        byte[] end
    )
        where T : IStructuralReadWrite, new()
    {
        return Iter<T>(
            (tx, instance) => tx.IndexScanRange(instance, index, prefixElems, prefix, start, end)
        );
    }

    internal uint IndexDeleteRange(
        string index,
        ushort prefixElems,
        byte[] prefix,
        byte[] start,
        byte[] end
    )
    {
        return Execute(
            (tx, instance) => tx.IndexDeleteRange(instance, index, prefixElems, prefix, start, end)
        );
    }

    internal byte[] Update(string index, byte[] row)
    {
        var name = table;
        return Execute((tx, instance) => tx.Update(instance, name, index, row));
    }

    private IEnumerable<T> Iter<T>(Func<IDatabaseTransaction, int, byte[]> scan)
        where T : IStructuralReadWrite, new()
    {
        var snapshot = Execute(scan);
        using var reader = new BinaryReader(new MemoryStream(snapshot));
        // The native snapshot is BSATN array<array<u8>>. Decode each framed row
        // directly from it rather than allocating another byte array per row.
        var count = reader.ReadUInt32();
        for (uint i = 0; i < count; i++)
        {
            access!.CheckActive();
            var length = reader.ReadUInt32();
            var end = reader.BaseStream.Position + length;
            var row = IStructuralReadWrite.Read<T>(reader);
            if (reader.BaseStream.Position != end)
                throw new InvalidDataException("Native table row does not match its BSATN schema.");
            yield return row;
        }
        access!.CheckActive();
        if (reader.BaseStream.Position != reader.BaseStream.Length)
            throw new InvalidDataException("Trailing bytes in native table snapshot.");
    }
}
#endif
