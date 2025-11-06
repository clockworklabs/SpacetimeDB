namespace SpacetimeDB.Internal;

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using SpacetimeDB.BSATN;

public abstract class IndexBase<Row>
    where Row : IStructuralReadWrite, new()
{
    internal readonly FFI.IndexId indexId;

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
        FFI.datastore_delete_by_index_scan_range_bsatn(
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
            FFI.datastore_index_scan_range_bsatn(
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

public abstract class ReadOnlyIndexBase<Row>(string name) : IndexBase<Row>(name)
    where Row : IStructuralReadWrite, new()
{
    protected IEnumerable<Row> Filter<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds => DoFilter(bounds);
}

public abstract class UniqueIndex<Handle, Row, T, RW>(string name) : IndexBase<Row>(name)
    where Handle : ITableView<Handle, Row>
    where Row : IStructuralReadWrite, new()
    where RW : struct, BSATN.IReadWrite<T>
{
    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    protected IEnumerable<Row> DoFilter(T key) => DoFilter(ToBounds(key));

    public bool Delete(T key) => DoDelete(ToBounds(key)) > 0;

    protected Row DoUpdate(Row row)
    {
        // Insert the row.
        var bytes = IStructuralReadWrite.ToBytes(row);
        var bytes_len = (uint)bytes.Length;
        FFI.datastore_update_bsatn(ITableView<Handle, Row>.tableId, indexId, bytes, ref bytes_len);

        return ITableView<Handle, Row>.IntegrateGeneratedColumns(row, bytes, bytes_len);
    }
}

public abstract class ReadOnlyUniqueIndex<Handle, Row, T, RW>(string name)
    : ReadOnlyIndexBase<Row>(name)
    where Handle : ReadOnlyTableView<Row>
    where Row : IStructuralReadWrite, new()
    where RW : struct, BSATN.IReadWrite<T>
{
    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    protected IEnumerable<Row> Filter(T key) => Filter(ToBounds(key));

    protected Row? FindSingle(T key) => Filter(key).Cast<Row?>().SingleOrDefault();
}

public abstract class ReadOnlyTableView<Row>
    where Row : IStructuralReadWrite, new()
{
    private readonly FFI.TableId tableId;

    private sealed class TableIter(FFI.TableId tableId) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_table_scan_bsatn(tableId, out handle);
    }

    protected ReadOnlyTableView(string tableName)
    {
        var nameBytes = Encoding.UTF8.GetBytes(tableName);
        FFI.table_id_from_name(nameBytes, (uint)nameBytes.Length, out tableId);
    }

    protected ulong DoCount()
    {
        FFI.datastore_table_row_count(tableId, out var count);
        return count;
    }

    protected IEnumerable<Row> DoIter() => new TableIter(tableId).Parse();
}
