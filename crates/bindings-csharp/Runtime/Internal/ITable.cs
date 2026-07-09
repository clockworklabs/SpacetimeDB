namespace SpacetimeDB.Internal;

using System.Buffers;
using System.Collections;
using SpacetimeDB.BSATN;

internal abstract class RawTableIterBase<T> : IEnumerable<T>
    where T : IStructuralReadWrite, new()
{
    private const int InitialBufferSize = 1024;

    protected abstract void IterStart(out FFI.RowIter handle);

    public IEnumerator<T> GetEnumerator()
    {
        IterStart(out var handle);
        var buffer = ArrayPool<byte>.Shared.Rent(InitialBufferSize);
        try
        {
            while (handle != FFI.RowIter.INVALID)
            {
                var requested_len = (uint)buffer.Length;
                var buffer_len = requested_len;
                var ret = FFI.row_iter_bsatn_advance(handle, buffer, ref buffer_len);
                if (ret == Errno.EXHAUSTED)
                {
                    handle = FFI.RowIter.INVALID;
                    if (buffer_len == requested_len)
                    {
                        buffer_len = 0;
                    }
                }

                // On success, the only way `buffer_len == 0` is for the iterator to be exhausted.
                // This happens when the host iterator was empty from the start.
                System.Diagnostics.Debug.Assert(!(ret == Errno.OK && buffer_len == 0));
                switch (ret)
                {
                    case Errno.EXHAUSTED
                    or Errno.OK:
                    {
                        using var stream = new MemoryStream(
                            buffer,
                            0,
                            (int)buffer_len,
                            writable: false,
                            publiclyVisible: true
                        );
                        using var reader = new BinaryReader(stream);
                        while (stream.Position < stream.Length)
                        {
                            yield return IStructuralReadWrite.Read<T>(reader);
                        }
                        break;
                    }
                    case Errno.BUFFER_TOO_SMALL:
                        ArrayPool<byte>.Shared.Return(buffer);
                        buffer = ArrayPool<byte>.Shared.Rent((int)buffer_len);
                        break;
                    default:
                        ret.Check();
                        break;
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
    static abstract RawTableDefV10 MakeTableDesc(ITypeRegistrar registrar);

    static abstract RawScheduleDefV10? MakeScheduleDesc();

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

#pragma warning disable IDE1006 // Used by static interface member call sites.
    internal static FFI.TableId tableId => tableId_.Value;
#pragma warning restore IDE1006

    ulong Count { get; }

    IEnumerable<T> Iter();

    T Insert(T row);

    bool Delete(T row);

    ulong Clear();

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

        return IntegrateGeneratedColumns(row, bytes, bytes_len);
    }

    // Writes back any generated column values.
    static T IntegrateGeneratedColumns(T row, byte[] bytes, uint gen_len)
    {
        using var stream = new MemoryStream(bytes, 0, (int)gen_len);
        using var reader = new BinaryReader(stream);
        return View.ReadGenFields(reader, row);
    }

    protected static bool DoDelete(T row)
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        // `datastore_delete_all_by_eq_bsatn` expects an array-like BSATN.
        // Write a length of 1 without actually wrapping the `row` into an array
        // (annoyingly, that would require passing `TRW` through a bunch of APIs).
        writer.Write(1U);
        row.WriteFields(writer);
        FFI.datastore_delete_all_by_eq_bsatn(
            tableId,
            stream.GetBuffer(),
            (uint)stream.Length,
            out var out_
        );
        return out_ > 0;
    }

    protected static ulong DoClear()
    {
        FFI.datastore_clear(tableId, out var count);
        return count;
    }

    protected static RawScheduleDefV10 MakeSchedule(string reducerName, ushort colIndex) =>
        new(
            SourceName: null,
            TableName: tableName,
            ScheduleAtCol: colIndex,
            FunctionName: reducerName
        );

    protected static RawSequenceDefV10 MakeSequence(ushort colIndex) =>
        new(
            SourceName: null,
            Column: colIndex,
            Start: null,
            MinValue: null,
            MaxValue: null,
            Increment: 1
        );

    protected static RawConstraintDefV10 MakeUniqueConstraint(ushort colIndex) =>
        new(SourceName: null, Data: new RawConstraintDataV9.Unique(new([colIndex])));
}
