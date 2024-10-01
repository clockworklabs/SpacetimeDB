namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

public interface ITable<T> : IStructuralReadWrite
    where T : ITable<T>, new()
{
    // These are the methods that codegen needs to implement.
    static abstract IEnumerable<TableDesc> MakeTableDesc(ITypeRegistrar registrar);
}

public interface ITableView<View, T>
    where View : ITableView<View, T>
    where T : ITable<T>, new()
{
    static abstract T ReadGenFields(BinaryReader reader, T row);

    // These are static helpers that codegen can use.
    private abstract class RawTableIterBase
    {
        public class Enumerator(FFI.RowIter handle) : IDisposable
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
                    // Avoid running ~RowIter if Dispose was executed successfully.
                    GC.SuppressFinalize(this);
                }
            }

            // Free unmanaged resource just in case user hasn't disposed for some reason.
            ~Enumerator()
            {
                // we already guard against double-free in Dispose.
                Dispose();
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

    private class RawTableIter(FFI.TableId tableId) : RawTableIterBase
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI.datastore_table_scan_bsatn(tableId, out handle);
    }

    private class RawTableIterByColEq(FFI.TableId tableId, FFI.ColId colId, byte[] value)
        : RawTableIterBase
    {
        protected override void IterStart(out FFI.RowIter handle)
        {
            _ = (tableId, colId, value);
            // Needs to be implemented via datastore_table_scan_bsatn
            throw new NotImplementedException();
        }
    }

    // Note: this must be Lazy to ensure that we don't try to get the tableId during startup, before the module is initialized.
    private static readonly Lazy<FFI.TableId> tableId_ =
        new(() =>
        {
            var name_bytes = System.Text.Encoding.UTF8.GetBytes(typeof(View).Name);
            FFI.table_id_from_name(name_bytes, (uint)name_bytes.Length, out var out_);
            return out_;
        });

    private static FFI.TableId tableId => tableId_.Value;

    public static IEnumerable<T> Iter() => new RawTableIter(tableId).Parse();

    protected static T Insert(T row)
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

    protected readonly ref struct ColEq
    {
        private readonly FFI.ColId colId;
        private readonly byte[] value;

        private ColEq(FFI.ColId colId, byte[] value)
        {
            this.colId = colId;
            this.value = value;
        }

        public static ColEq Where<TCol, TColRW>(ushort colId, TCol colValue, TColRW rw)
            where TColRW : IReadWrite<TCol>
        {
            return new(new FFI.ColId(colId), IStructuralReadWrite.ToBytes(rw, colValue));
        }

        // Note: do not inline FindBy from the Codegen as a helper API here.
        // C# handles nullables on generics in a weird way, and will break if [SpacetimeDB.Type] is used on a struct.
        public IEnumerable<T> Iter() => new RawTableIterByColEq(tableId, colId, value).Parse();

        // Needs to be implemented via `datastore_delete_all_by_eq_bsatn`.
        public bool Delete() => throw new NotImplementedException();

        public bool Update(T row)
        {
            // Just like in Rust bindings, updating is just deleting and inserting for now.
            if (!Delete())
            {
                return false;
            }
            Insert(row);
            return true;
        }
    }
}
