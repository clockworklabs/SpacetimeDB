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

    private delegate FFI.CheckedStatus FfiCallWithBounds<T>(
        FFI.IndexId index_id,
        ReadOnlySpan<byte> prefix,
        uint prefix_len,
        FFI.ColId prefix_elems,
        ReadOnlySpan<byte> rstart,
        uint rstart_len,
        ReadOnlySpan<byte> rend,
        uint rend_len,
        out T out_
    );

    private static void MakeFfiCallWithBounds<Bounds, T>(
        FFI.IndexId indexId,
        Bounds bounds,
        FfiCallWithBounds<T> ffiCall,
        out T out_
    )
        where Bounds : IBTreeIndexBounds
    {
        var prefixElems = new FFI.ColId(bounds.PrefixElems);

        using var buffer = SerializationBuffer.Borrow();

        var w = buffer.Writer;
        bounds.Prefix(w);
        var prefix_idx = (int)buffer.Position;
        bounds.RStart(w);
        var rstart_idx = (int)buffer.Position;
        bounds.REnd(w);
        var bytes = buffer.GetWritten();

        var prefix = bytes[..prefix_idx];
        var rstart = bytes[prefix_idx..rstart_idx];
        var rend = bytes[rstart_idx..];

        ffiCall(
            indexId,
            prefix,
            (uint)prefix.Length,
            prefixElems,
            rstart,
            (uint)rstart.Length,
            rend,
            (uint)rend.Length,
            out out_
        );
    }

    protected IEnumerable<Row> DoFilter<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds => new RawTableIter<Bounds>(indexId, bounds).Parse();

    protected uint DoDelete<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds
    {
        MakeFfiCallWithBounds(
            indexId,
            bounds,
            FFI.datastore_delete_by_btree_scan_bsatn,
            out uint out_
        );
        return out_;
    }

    private class RawTableIter<Bounds>(FFI.IndexId indexId, Bounds bounds) : RawTableIterBase<Row>
        where Bounds : IBTreeIndexBounds
    {
        protected override void IterStart(out FFI.RowIter handle)
        {
            MakeFfiCallWithBounds(indexId, bounds, FFI.datastore_btree_scan_bsatn, out handle);
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

    protected bool DoUpdate(T key, ref Row row)
    {
        if (!Delete(key))
        {
            return false;
        }
        table.Insert(row);
        return true;
    }
}
