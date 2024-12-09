namespace SpacetimeDB.Internal;

using System;
using SpacetimeDB.BSATN;

public abstract class IndexBase<Row>
    where Row : IStructuralReadWrite, new()
{
    private readonly FFI.IndexId indexId;

    public IndexBase(string name)
    {
        var name_bytes = System.Text.Encoding.UTF8.GetBytes(name);
        FFI.index_id_from_name(name_bytes, (uint)name_bytes.Length, out indexId);
    }

    private static void ToParams<Bounds>(
        Bounds bounds,
        out FFI.ColId prefixElems,
        out ReadOnlySpan<byte> prefix,
        out ReadOnlySpan<byte> rstart,
        out ReadOnlySpan<byte> rend
    )
        where Bounds : IBTreeIndexBounds
    {
        prefixElems = new FFI.ColId(bounds.PrefixElems);

        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        bounds.Prefix(w);
        var prefix_idx = (int)s.Length;
        bounds.RStart(w);
        var rstart_idx = (int)s.Length;
        bounds.REnd(w);
        var rend_idx = (int)s.Length;

        var bytes = s.GetBuffer().AsSpan();
        prefix = bytes[..prefix_idx];
        rstart = bytes[prefix_idx..rstart_idx];
        rend = bytes[rstart_idx..rend_idx];
    }

    protected IEnumerable<Row> DoFilter<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds => new RawTableIter<Bounds>(indexId, bounds).Parse();

    protected uint DoDelete<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds
    {
        ToParams(bounds, out var prefixElems, out var prefix, out var rstart, out var rend);
        FFI.datastore_delete_by_btree_scan_bsatn(
            indexId,
            prefix,
            (uint)prefix.Length,
            prefixElems,
            rstart,
            (uint)rstart.Length,
            rend,
            (uint)rend.Length,
            out var out_
        );
        return out_;
    }

    private class RawTableIter<Bounds>(FFI.IndexId indexId, Bounds bounds) : RawTableIterBase<Row>
        where Bounds : IBTreeIndexBounds
    {
        protected override void IterStart(out FFI.RowIter handle)
        {
            ToParams(bounds, out var prefixElems, out var prefix, out var rstart, out var rend);
            FFI.datastore_btree_scan_bsatn(
                indexId,
                prefix,
                (uint)prefix.Length,
                prefixElems,
                rstart,
                (uint)rstart.Length,
                rend,
                (uint)rend.Length,
                out handle
            );
        }
    }
}

public abstract class UniqueIndex<Handle, Row, T, RW>(Handle table, string name)
    : IndexBase<Row>(name)
    where Handle : ITableView<Handle, Row>
    where Row : IStructuralReadWrite, new()
    where T : IEquatable<T>
    where RW : struct, BSATN.IReadWrite<T>
{
    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    protected IEnumerable<Row> DoFilter(T key) => DoFilter(ToBounds(key));

    public bool Delete(T key) => DoDelete(ToBounds(key)) > 0;

    protected bool DoUpdate(T key, Row row)
    {
        if (!Delete(key))
        {
            return false;
        }
        table.Insert(row);
        return true;
    }
}
