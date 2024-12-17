namespace SpacetimeDB.Internal;

using System.Buffers;
using System.Collections;
using SpacetimeDB.BSATN;

internal abstract class RawTableIterBase<T> : IEnumerable<T>
    where T : IStructuralReadWrite, new()
{
    protected abstract void IterStart(out FFI.RowIter handle);

    public IEnumerator<T> GetEnumerator()
    {
        IterStart(out var handle);
        // Initial buffer size to match Rust one (see `DEFAULT_BUFFER_CAPACITY` in `bindings/src/lib.rs`).
        // Use pool to reduce GC pressure between iterations.
        var buffer = ArrayPool<byte>.Shared.Rent(0x10_000);
        try
        {
            while (handle != FFI.RowIter.INVALID)
            {
                var buffer_len = (uint)buffer.Length;
                var ret = FFI.row_iter_bsatn_advance(handle, buffer, ref buffer_len);
                // On success, the only way `buffer_len == 0` is for the iterator to be exhausted.
                // This happens when the host iterator was empty from the start.
                System.Diagnostics.Debug.Assert(!(ret == Errno.OK && buffer_len == 0));
                switch (ret)
                {
                    // Iterator is exhausted.
                    // Treat in the same way as OK, just tell the next iteration to stop.
                    case Errno.EXHAUSTED:
                        handle = FFI.RowIter.INVALID;
                        goto case Errno.OK;
                    // We got a chunk of rows, parse all of them before moving to the next chunk.
                    case Errno.OK:
                    {
                        using var stream = new MemoryStream(buffer, 0, (int)buffer_len);
                        using var reader = new BinaryReader(stream);
                        while (stream.Position < stream.Length)
                        {
                            yield return IStructuralReadWrite.Read<T>(reader);
                        }
                        break;
                    }
                    // Couldn't find the iterator, error!
                    case Errno.NO_SUCH_ITER:
                        throw new NoSuchIterException();
                    // The scratch `buffer` is too small to fit a row / chunk.
                    // Grow `buffer` and try again.
                    // The `buffer_len` will have been updated with the necessary size.
                    case Errno.BUFFER_TOO_SMALL:
                        ArrayPool<byte>.Shared.Return(buffer);
                        break;
                    default:
                        throw new UnknownException(ret);
                }
            }
        }
        finally
        {
            if (handle != FFI.RowIter.INVALID)
            {
                FFI.row_iter_bsatn_close(handle);
            }
            ArrayPool<byte>.Shared.Return(buffer);
        }
    }

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}

public interface ITableView<View, T>
    where View : ITableView<View, T>
    where T : IStructuralReadWrite, new()
{
    // These are the methods that codegen needs to implement.
    static abstract RawTableDefV9 MakeTableDesc(ITypeRegistrar registrar);

    static abstract T ReadGenFields(BinaryReader reader, T row);

    // These are static helpers that codegen can use.

    private class RawTableIter(FFI.TableId tableId) : RawTableIterBase<T>
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_table_scan_bsatn(tableId, out handle);
    }

    private static readonly string tableName = typeof(View).Name;

    // Note: this must be Lazy to ensure that we don't try to get the tableId during startup, before the module is initialized.
    private static readonly Lazy<FFI.TableId> tableId_ =
        new(() =>
        {
            var name_bytes = System.Text.Encoding.UTF8.GetBytes(tableName);
            FFI.table_id_from_name(name_bytes, (uint)name_bytes.Length, out var out_);
            return out_;
        });

    private static FFI.TableId tableId => tableId_.Value;

    ulong Count { get; }

    IEnumerable<T> Iter();

    T Insert(T row);

    bool Delete(T row);

    protected static ulong DoCount()
    {
        FFI.datastore_table_row_count(tableId, out var count);
        return count;
    }

    protected static IEnumerable<T> DoIter() => new RawTableIter(tableId);

    protected static T DoInsert(T row)
    {
        // Insert the row.
        var bytes = IStructuralReadWrite.ToBytes(row);
        var bytes_len = (uint)bytes.Length;
        FFI.datastore_insert_bsatn(tableId, bytes, ref bytes_len);

        // Write back any generated column values.
        using var stream = new MemoryStream(bytes, 0, (int)bytes_len);
        using var reader = new BinaryReader(stream);
        return View.ReadGenFields(reader, row);
    }

    protected static bool DoDelete(T row)
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        FFI.datastore_delete_all_by_eq_bsatn(tableId, bytes, (uint)bytes.Length, out var out_);
        return out_ > 0;
    }

    protected static RawScheduleDefV9 MakeSchedule(string reducerName, ushort colIndex) =>
        new(Name: $"{tableName}_sched", ReducerName: reducerName, ScheduledAtColumn: colIndex);

    protected static RawSequenceDefV9 MakeSequence(ushort colIndex) =>
        new(
            Name: null,
            Column: colIndex,
            Start: null,
            MinValue: null,
            MaxValue: null,
            Increment: 1
        );

    protected static RawConstraintDefV9 MakeUniqueConstraint(ushort colIndex) =>
        new(Name: null, Data: new RawConstraintDataV9.Unique(new([colIndex])));
}
