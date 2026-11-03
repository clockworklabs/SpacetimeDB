#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

/// <summary>Ownership carried by generated table and index handles.</summary>
public readonly struct TableAccessor
{
    private readonly DatabaseAccess? access;
    private readonly string assemblyIdentity;
    private readonly string table;

    public TableAccessor(Local? db, string assemblyIdentity, string table)
        : this(db?.Access, assemblyIdentity, table) { }

    public TableAccessor(LocalReadOnly? db, string assemblyIdentity, string table)
        : this(db?.Access, assemblyIdentity, table) { }

    private TableAccessor(DatabaseAccess? access, string assemblyIdentity, string table)
    {
        this.access = access;
        this.assemblyIdentity = assemblyIdentity;
        this.table = table;
    }

    private TResult Execute<TResult>(Func<IDatabaseTransaction, TResult> body)
    {
        var access = DatabaseAccess.Require(this.access);
        access.CheckActive();
        if (access.Transaction is { } transaction)
            return body(transaction);
        using var standalone = access.Backend.BeginTransaction();
        var result = body(standalone);
        standalone.Commit();
        return result;
    }

    internal ulong Count()
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Execute(tx => tx.Count(assembly, name));
    }

    internal byte[] Insert(byte[] row)
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Execute(tx => tx.Insert(assembly, name, row));
    }

    internal bool Delete(byte[] row)
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Execute(tx => tx.Delete(assembly, name, row));
    }

    internal ulong Clear()
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Execute(tx => tx.Clear(assembly, name));
    }

    internal IEnumerable<T> Iter<T>()
        where T : IStructuralReadWrite, new()
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Iter<T>(tx => tx.Scan(assembly, name));
    }

    internal IEnumerable<T> IndexScanPoint<T>(string index, byte[] point)
        where T : IStructuralReadWrite, new()
    {
        var assembly = assemblyIdentity;
        return Iter<T>(tx => tx.IndexScanPoint(assembly, index, point));
    }

    internal bool IndexDeletePoint(string index, byte[] point)
    {
        var assembly = assemblyIdentity;
        return Execute(tx => tx.IndexDeletePoint(assembly, index, point));
    }

    internal byte[] Update(string index, byte[] row)
    {
        var (assembly, name) = (assemblyIdentity, table);
        return Execute(tx => tx.Update(assembly, name, index, row));
    }

    private IEnumerable<T> Iter<T>(Func<IDatabaseTransaction, byte[]> scan)
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

    internal TResult RejectUnsupported<TResult>(string operation)
    {
        DatabaseAccess.Require(access).CheckActive();
        throw new NotSupportedException(
            $"Managed module testing does not yet support {operation}."
        );
    }
}
#endif
