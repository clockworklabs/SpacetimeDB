namespace SpacetimeDB.Internal;

using System.Diagnostics;
using System.Text;
using SpacetimeDB.BSATN;

// SpacetimeDB modules are guaranteed to be single-threaded, and, unlike in iterators, we don't have any potential for single-threaded concurrency.
// This means we can use a single static buffer for all serialization operations, as long as we don't use it concurrently (e.g. in iterators).
// In order to ensure that, this struct acts as a singleton guard to the data that actually lives in a static scope.
public class SerializationBuffer : MemoryStream
{
    public readonly BinaryReader Reader;
    public readonly BinaryWriter Writer;
    private bool isUsed = false;

    private SerializationBuffer()
    {
        Reader = new(this, Encoding.UTF8, leaveOpen: true);
        Writer = new(this, Encoding.UTF8, leaveOpen: true);
    }

    private static readonly SerializationBuffer instance = new();

    public static SerializationBuffer Borrow()
    {
        Debug.Assert(!instance.isUsed, "Buffer is already in use");
        instance.isUsed = true;
        instance.Position = 0;
        return instance;
    }

    public Span<byte> GetWritten() => GetBuffer().AsSpan(0, (int)Position);

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            isUsed = false;
        }
    }
}

internal abstract class RawTableIterBase<T>
    where T : IStructuralReadWrite, new()
{
    public sealed class Enumerator(FFI.RowIter handle) : IDisposable
    {
        byte[] buffer = new byte[0x20_000];
        public byte[] Current { get; private set; } = [];

        public bool MoveNext()
        {
            if (handle == FFI.RowIter.INVALID)
            {
                return false;
            }

            uint buffer_len;
            while (true)
            {
                buffer_len = (uint)buffer.Length;
                var ret = FFI.row_iter_bsatn_advance(handle, buffer, ref buffer_len);
                if (ret == Errno.EXHAUSTED)
                {
                    handle = FFI.RowIter.INVALID;
                }
                // On success, the only way `buffer_len == 0` is for the iterator to be exhausted.
                // This happens when the host iterator was empty from the start.
                System.Diagnostics.Debug.Assert(!(ret == Errno.OK && buffer_len == 0));
                switch (ret)
                {
                    // Iterator advanced and may also be `EXHAUSTED`.
                    // When `OK`, we'll need to advance the iterator in the next call to `MoveNext`.
                    // In both cases, copy over the row data to `Current` from the scratch `buffer`.
                    case Errno.EXHAUSTED
                    or Errno.OK:
                        Current = new byte[buffer_len];
                        Array.Copy(buffer, 0, Current, 0, buffer_len);
                        return buffer_len != 0;
                    // Couldn't find the iterator, error!
                    case Errno.NO_SUCH_ITER:
                        throw new NoSuchIterException();
                    // The scratch `buffer` is too small to fit a row / chunk.
                    // Grow `buffer` and try again.
                    // The `buffer_len` will have been updated with the necessary size.
                    case Errno.BUFFER_TOO_SMALL:
                        buffer = new byte[buffer_len];
                        continue;
                    default:
                        throw new UnknownException(ret);
                }
            }
        }

        public void Dispose()
        {
            if (handle != FFI.RowIter.INVALID)
            {
                FFI.row_iter_bsatn_close(handle);
                handle = FFI.RowIter.INVALID;
            }
        }

        public void Reset()
        {
            throw new NotImplementedException();
        }
    }

    protected abstract void IterStart(out FFI.RowIter handle);

    // Note: using the GetEnumerator() duck-typing protocol instead of IEnumerable to avoid extra boxing.
    public Enumerator GetEnumerator()
    {
        IterStart(out var handle);
        return new(handle);
    }

    public IEnumerable<T> Parse()
    {
        foreach (var chunk in this)
        {
            using var stream = new MemoryStream(chunk);
            using var reader = new BinaryReader(stream);
            while (stream.Position < stream.Length)
            {
                yield return IStructuralReadWrite.Read<T>(reader);
            }
        }
    }
}

public interface ITableView<View, T>
    where View : ITableView<View, T>
    where T : IStructuralReadWrite, new()
{
    // These are the methods that codegen needs to implement.
    static abstract RawTableDefV9 MakeTableDesc(ITypeRegistrar registrar);

    static abstract void ReadGenFields(BinaryReader reader, ref T row);

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

    protected static IEnumerable<T> DoIter() => new RawTableIter(tableId).Parse();

    protected static void DoInsert(ref T row)
    {
        using var buffer = SerializationBuffer.Borrow();

        // Insert the row.
        row.WriteFields(buffer.Writer);
        var bytes = buffer.GetWritten();
        var bytes_len = (uint)bytes.Length;
        FFI.datastore_insert_bsatn(tableId, bytes, ref bytes_len);

        // Read back any generated column values.
        buffer.Position = 0;
        View.ReadGenFields(buffer.Reader, ref row);
        Debug.Assert(buffer.Position == bytes_len);
    }

    protected static bool DoDelete(in T row)
    {
        using var buffer = SerializationBuffer.Borrow();
        row.WriteFields(buffer.Writer);
        var bytes = buffer.GetWritten();
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
