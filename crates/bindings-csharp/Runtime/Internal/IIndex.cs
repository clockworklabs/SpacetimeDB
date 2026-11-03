namespace SpacetimeDB.Internal;

using System;
using System.Collections.Generic;
using System.IO;
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
using System.Text;
#endif
using SpacetimeDB.BSATN;

// Managed builds have additional constructors; keep the declarations shared with WASM.
#pragma warning disable IDE0290

public abstract class IndexBase<Row>
    where Row : IStructuralReadWrite, new()
{
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private readonly FFI.IndexId hostIndexId;
#endif
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    private readonly TableAccessor accessor;
    private readonly string indexName;

    protected IndexBase(TableAccessor accessor, string name)
    {
        this.accessor = accessor;
        indexName = name;
    }

    protected IEnumerable<Row> ScanPoint(byte[] point) =>
        accessor.IndexScanPoint<Row>(indexName, point);

    protected bool DeletePoint(byte[] point) => accessor.IndexDeletePoint(indexName, point);

    protected byte[] UpdateRow(byte[] row) => accessor.Update(indexName, row);
#endif

    internal FFI.IndexId IndexId
    {
        get
        {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
            return accessor.RejectUnsupported<FFI.IndexId>("range index operations");
#else
            return hostIndexId;
#endif
        }
    }

    public IndexBase(string name)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new InvalidOperationException("Managed indexes require a bound table accessor.");
#else
        var name_bytes = System.Text.Encoding.UTF8.GetBytes(name);
        FFI.index_id_from_name(name_bytes, name_bytes.Length, out hostIndexId);
#endif
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
        where Bounds : IBTreeIndexBounds => new RawTableIter<Bounds>(IndexId, bounds);

    protected uint DoDelete<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds
    {
        ToParams(bounds, out var prefixElems, out var prefix, out var rstart, out var rend);
        FFI.datastore_delete_by_index_scan_range_bsatn(
            IndexId,
            prefix,
            prefix.Length,
            prefixElems,
            rstart,
            rstart.Length,
            rend,
            rend.Length,
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
                prefix.Length,
                prefixElems,
                rstart,
                rstart.Length,
                rend,
                rend.Length,
                out handle
            );
        }
    }
}

public abstract class ReadOnlyIndexBase<Row> : IndexBase<Row>
    where Row : IStructuralReadWrite, new()
{
    public ReadOnlyIndexBase(string name)
        : base(name) { }

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected ReadOnlyIndexBase(TableAccessor accessor, string name)
        : base(accessor, name) { }
#endif

    protected IEnumerable<Row> Filter<Bounds>(Bounds bounds)
        where Bounds : IBTreeIndexBounds => DoFilter(bounds);
}

public abstract class UniqueIndex<Handle, Row, T, RW> : IndexBase<Row>
    where Handle : ITableView<Handle, Row>
    where Row : struct, IStructuralReadWrite
    where RW : struct, BSATN.IReadWrite<T>
{
    public UniqueIndex(string name)
        : base(name) { }

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected UniqueIndex(TableAccessor accessor, string name)
        : base(accessor, name) { }
#endif

    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    private sealed class RawPointIter(FFI.IndexId indexId, byte[] point) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_index_scan_point_bsatn(indexId, point, point.Length, out handle);
    }

    protected IEnumerable<Row> DoFilter(T key) => DoFilter(ToBounds(key));

    public bool Delete(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        return DeletePoint(point);
#else
        FFI.datastore_delete_by_index_scan_point_bsatn(
            IndexId,
            point,
            point.Length,
            out var numDeleted
        );
        return numDeleted > 0;
#endif
    }

    protected Row? FindSingle(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        using var e = ScanPoint(point).GetEnumerator();
#else
        using var e = new RawPointIter(IndexId, point).GetEnumerator();
#endif
        if (!e.MoveNext())
        {
            return null;
        }

        var row = e.Current;
        if (e.MoveNext())
        {
            throw new InvalidOperationException("Unique index point scan returned >1 rows");
        }

        return row;
    }

    protected Row DoUpdate(Row row)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        var bytes = IStructuralReadWrite.ToBytes(row);
        var generated = UpdateRow(bytes);
        return ITableView<Handle, Row>.IntegrateGeneratedColumns(row, generated, generated.Length);
#else
        return UpdateRow(row, ITableView<Handle, Row>.tableId);
#endif
    }

    protected Row DoUpdate(Row row, TableHandle table) =>
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        DoUpdate(row);
#else
        UpdateRow(row, table.Id);
#endif

#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private Row UpdateRow(Row row, FFI.TableId tableId)
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        var bytes_len = bytes.Length;
        var id = IndexId;
        FFI.datastore_update_bsatn(tableId, id, bytes, ref bytes_len);

        return ITableView<Handle, Row>.IntegrateGeneratedColumns(row, bytes, bytes_len);
    }
#endif
}

public abstract class RefUniqueIndex<Handle, Row, T, RW> : IndexBase<Row>
    where Handle : ITableView<Handle, Row>
    where Row : class, IStructuralReadWrite, new()
    where RW : struct, BSATN.IReadWrite<T>
{
    public RefUniqueIndex(string name)
        : base(name) { }

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected RefUniqueIndex(TableAccessor accessor, string name)
        : base(accessor, name) { }
#endif

    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    private sealed class RawPointIter(FFI.IndexId indexId, byte[] point) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_index_scan_point_bsatn(indexId, point, point.Length, out handle);
    }

    protected IEnumerable<Row> DoFilter(T key) => DoFilter(ToBounds(key));

    public bool Delete(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        return DeletePoint(point);
#else
        FFI.datastore_delete_by_index_scan_point_bsatn(
            IndexId,
            point,
            point.Length,
            out var numDeleted
        );
        return numDeleted > 0;
#endif
    }

    protected Row? FindSingle(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        using var e = ScanPoint(point).GetEnumerator();
#else
        using var e = new RawPointIter(IndexId, point).GetEnumerator();
#endif
        if (!e.MoveNext())
        {
            return null;
        }

        var row = e.Current;
        if (e.MoveNext())
        {
            throw new InvalidOperationException("Unique index point scan returned >1 rows");
        }

        return row;
    }

    protected Row DoUpdate(Row row)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        var bytes = IStructuralReadWrite.ToBytes(row);
        var generated = UpdateRow(bytes);
        return ITableView<Handle, Row>.IntegrateGeneratedColumns(row, generated, generated.Length);
#else
        return UpdateRow(row, ITableView<Handle, Row>.tableId);
#endif
    }

    protected Row DoUpdate(Row row, TableHandle table) =>
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        DoUpdate(row);
#else
        UpdateRow(row, table.Id);
#endif

#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private Row UpdateRow(Row row, FFI.TableId tableId)
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        var bytes_len = bytes.Length;
        var id = IndexId;
        FFI.datastore_update_bsatn(tableId, id, bytes, ref bytes_len);

        return ITableView<Handle, Row>.IntegrateGeneratedColumns(row, bytes, bytes_len);
    }
#endif
}

public abstract class ReadOnlyUniqueIndex<Handle, Row, T, RW> : ReadOnlyIndexBase<Row>
    where Handle : ReadOnlyTableView<Row>
    where Row : struct, IStructuralReadWrite
    where RW : struct, BSATN.IReadWrite<T>
{
    public ReadOnlyUniqueIndex(string name)
        : base(name) { }

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected ReadOnlyUniqueIndex(TableAccessor accessor, string name)
        : base(accessor, name) { }
#endif

    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    private sealed class RawPointIter(FFI.IndexId indexId, byte[] point) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_index_scan_point_bsatn(indexId, point, point.Length, out handle);
    }

    protected IEnumerable<Row> Filter(T key) => Filter(ToBounds(key));

    protected Row? FindSingle(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        using var e = ScanPoint(point).GetEnumerator();
#else
        using var e = new RawPointIter(IndexId, point).GetEnumerator();
#endif
        if (!e.MoveNext())
        {
            return null;
        }

        var row = e.Current;
        if (e.MoveNext())
        {
            throw new InvalidOperationException("Unique index point scan returned >1 rows");
        }

        return row;
    }
}

public abstract class ReadOnlyRefUniqueIndex<Handle, Row, T, RW> : ReadOnlyIndexBase<Row>
    where Handle : ReadOnlyTableView<Row>
    where Row : class, IStructuralReadWrite, new()
    where RW : struct, BSATN.IReadWrite<T>
{
    public ReadOnlyRefUniqueIndex(string name)
        : base(name) { }

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected ReadOnlyRefUniqueIndex(TableAccessor accessor, string name)
        : base(accessor, name) { }
#endif

    private static BTreeIndexBounds<T, RW> ToBounds(T key) => new(key);

    private sealed class RawPointIter(FFI.IndexId indexId, byte[] point) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_index_scan_point_bsatn(indexId, point, point.Length, out handle);
    }

    protected IEnumerable<Row> Filter(T key) => Filter(ToBounds(key));

    protected Row? FindSingle(T key)
    {
        using var s = new MemoryStream();
        using var w = new BinaryWriter(s);
        new RW().Write(w, key);
        var point = s.ToArray();

#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        using var e = ScanPoint(point).GetEnumerator();
#else
        using var e = new RawPointIter(IndexId, point).GetEnumerator();
#endif
        if (!e.MoveNext())
        {
            return null;
        }

        var row = e.Current;
        if (e.MoveNext())
        {
            throw new InvalidOperationException("Unique index point scan returned >1 rows");
        }

        return row;
    }
}

public abstract class ReadOnlyTableView<Row>
    where Row : IStructuralReadWrite, new()
{
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private readonly FFI.TableId tableId;
#endif
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
    protected readonly TableAccessor __access;

    protected ReadOnlyTableView(TableAccessor accessor)
    {
        __access = accessor;
    }
#endif

    private sealed class TableIter(FFI.TableId tableId) : RawTableIterBase<Row>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_table_scan_bsatn(tableId, out handle);
    }

    protected ReadOnlyTableView(string tableName)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new InvalidOperationException("Managed tables require a bound table accessor.");
#else
        var nameBytes = Encoding.UTF8.GetBytes(tableName);
        FFI.table_id_from_name(nameBytes, nameBytes.Length, out tableId);
#endif
    }

    protected ulong DoCount()
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        return __access.Count();
#else
        FFI.datastore_table_row_count(tableId, out var count);
        return count;
#endif
    }

    protected IEnumerable<Row> DoIter() =>
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        __access.Iter<Row>();
#else
            new TableIter(tableId);
#endif
}
