namespace SpacetimeDB.Internal;

using System.Linq.Expressions;
using SpacetimeDB.BSATN;

public interface ITable<T> : IStructuralReadWrite
    where T : ITable<T>, new()
{
    // These are the methods that codegen needs to implement.
    static abstract Module.TableDesc MakeTableDesc(ITypeRegistrar registrar);
    static abstract Filter CreateFilter();
    static abstract bool HasAutoIncFields { get; }

    // These are static helpers that codegen can use.

    private abstract class RawTableIterBase
    {
        public class Enumerator(FFI.RowIter handle) : IDisposable
        {
            byte[] buffer = new byte[0x20_000];
            public byte[] Current { get; private set; } = [];

            public bool MoveNext()
            {
                uint buffer_len;
                while (true)
                {
                    buffer_len = (uint)buffer.Length;
                    try
                    {
                        FFI._iter_advance(handle, buffer, ref buffer_len);
                    }
                    catch (BufferTooSmallException)
                    {
                        buffer = new byte[buffer_len];
                        continue;
                    }
                    break;
                }
                Current = new byte[buffer_len];
                Array.Copy(buffer, 0, Current, 0, buffer_len);
                return buffer_len != 0;
            }

            public void Dispose()
            {
                if (!handle.Equals(FFI.RowIter.INVALID))
                {
                    FFI._iter_drop(handle);
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
                    yield return Read<T>(reader);
                }
            }
        }
    }

    private class RawTableIter(FFI.TableId tableId) : RawTableIterBase
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI._iter_start(tableId, out handle);
    }

    private class RawTableIterFiltered(FFI.TableId tableId, byte[] filterBytes) : RawTableIterBase
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI._iter_start_filtered(tableId, filterBytes, (uint)filterBytes.Length, out handle);
    }

    private class RawTableIterByColEq(FFI.TableId tableId, FFI.ColId colId, byte[] value)
        : RawTableIterBase
    {
        protected override void IterStart(out FFI.RowIter handle) =>
            FFI._iter_by_col_eq(tableId, colId, value, (uint)value.Length, out handle);
    }

    // Note: this must be Lazy to ensure that we don't try to get the tableId during startup, before the module is initialized.
    private static readonly Lazy<FFI.TableId> tableId_ =
        new(() =>
        {
            var name_bytes = System.Text.Encoding.UTF8.GetBytes(typeof(T).Name);
            FFI._get_table_id(name_bytes, (uint)name_bytes.Length, out var out_);
            return out_;
        });

    private static FFI.TableId tableId => tableId_.Value;

    public static IEnumerable<T> Iter() => new RawTableIter(tableId).Parse();

    private static readonly Lazy<Filter> filter = new(T.CreateFilter);

    public static IEnumerable<T> Query(Expression<Func<T, bool>> query) =>
        new RawTableIterFiltered(tableId, filter.Value.Compile(query)).Parse();

    protected static void Insert(T row)
    {
        var bytes = ToBytes(row);
        FFI._insert(tableId, bytes, (uint)bytes.Length);
        if (T.HasAutoIncFields)
        {
            using var stream = new MemoryStream(bytes);
            using var reader = new BinaryReader(stream);
            row.ReadFields(reader);
        }
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

        public static ColEq Where<TCol, TColRW>(uint colId, TCol colValue, TColRW rw)
            where TColRW : IReadWrite<TCol>
        {
            return new(new FFI.ColId(colId), ToBytes(rw, colValue));
        }

        // Note: do not inline FindBy from the Codegen as a helper API here.
        // C# handles nullables on generics in a weird way, and will break if [SpacetimeDB.Type] is used on a struct.
        public IEnumerable<T> Iter() => new RawTableIterByColEq(tableId, colId, value).Parse();

        public bool Delete()
        {
            FFI._delete_by_col_eq(tableId, colId, value, (uint)value.Length, out var out_);
            return out_ > 0;
        }

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
