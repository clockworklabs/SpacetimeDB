namespace SpacetimeDB;

using System.Collections;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;
using static System.Text.Encoding;

public static partial class Runtime
{
    [SpacetimeDB.Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    internal static byte[] Consume(this RawBindings.Buffer buffer)
    {
        var len = RawBindings._buffer_len(buffer);
        var result = new byte[len];
        RawBindings._buffer_consume(buffer, result, len);
        return result;
    }

    private class RowIter(RawBindings.RowIter handle) : IEnumerator<byte[]>, IDisposable
    {
        private byte[] buffer = new byte[0x20_000];
        public byte[] Current { get; private set; } = [];

        object IEnumerator.Current => Current;

        public bool MoveNext()
        {
            uint buffer_len;
            while (true)
            {
                buffer_len = (uint)buffer.Length;
                try
                {
                    RawBindings._iter_advance(handle, buffer, ref buffer_len);
                }
                catch (RawBindings.BufferTooSmallException)
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
            if (!handle.Equals(RawBindings.RowIter.INVALID))
            {
                RawBindings._iter_drop(handle);
                handle = RawBindings.RowIter.INVALID;
                // Avoid running ~RowIter if Dispose was executed successfully.
                GC.SuppressFinalize(this);
            }
        }

        // Free unmanaged resource just in case user hasn't disposed for some reason.
        ~RowIter()
        {
            // we already guard against double-free in Dispose.
            Dispose();
        }

        public void Reset()
        {
            throw new NotImplementedException();
        }
    }

    public abstract class RawTableIterBase : IEnumerable<byte[]>
    {
        protected abstract void IterStart(out RawBindings.RowIter handle);

        public IEnumerator<byte[]> GetEnumerator()
        {
            IterStart(out var handle);
            return new RowIter(handle);
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        private static IEnumerable<T> ParseChunk<T>(byte[] chunk)
            where T : IStructuralReadWrite, new()
        {
            using var stream = new MemoryStream(chunk);
            using var reader = new BinaryReader(stream);
            while (stream.Position < stream.Length)
            {
                yield return IStructuralReadWrite.Read<T>(reader);
            }
        }

        public IEnumerable<T> Parse<T>()
            where T : IStructuralReadWrite, new()
        {
            return this.SelectMany(ParseChunk<T>);
        }
    }

    public class RawTableIter(RawBindings.TableId tableId) : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_start(tableId, out handle);
    }

    public class RawTableIterFiltered(RawBindings.TableId tableId, byte[] filterBytes)
        : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_start_filtered(
                tableId,
                filterBytes,
                (uint)filterBytes.Length,
                out handle
            );
    }

    public class RawTableIterByColEq(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value
    ) : RawTableIterBase
    {
        protected override void IterStart(out RawBindings.RowIter handle) =>
            RawBindings._iter_by_col_eq(tableId, colId, value, (uint)value.Length, out handle);
    }

    public enum LogLevel : byte
    {
        Error,
        Warn,
        Info,
        Debug,
        Trace,
        Panic
    }

    public static void Log(
        string text,
        LogLevel level = LogLevel.Info,
        [CallerMemberName] string target = "",
        [CallerFilePath] string filename = "",
        [CallerLineNumber] uint lineNumber = 0
    )
    {
        var target_bytes = UTF8.GetBytes(target);
        var filename_bytes = UTF8.GetBytes(filename);
        var text_bytes = UTF8.GetBytes(text);

        RawBindings._console_log(
            (byte)level,
            target_bytes,
            (uint)target_bytes.Length,
            filename_bytes,
            (uint)filename_bytes.Length,
            lineNumber,
            text_bytes,
            (uint)text_bytes.Length
        );
    }

    public readonly struct Identity(byte[] bytes) : IEquatable<Identity>
    {
        private readonly byte[] bytes = bytes;

        public bool Equals(Identity other) => bytes.SequenceEqual(other.bytes);

        public override bool Equals(object? obj) => obj is Identity other && Equals(other);

        public static bool operator ==(Identity left, Identity right) => left.Equals(right);

        public static bool operator !=(Identity left, Identity right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        // We need to implement this manually because `spacetime generate` only
        // recognises inline product type with special field name and
        // not types registered on ModuleDef (which is what [SpacetimeDB.Type] does).
        public readonly struct BSATN : IReadWrite<Identity>
        {
            public Identity Read(BinaryReader reader) => new(ByteArray.Instance.Read(reader));

            public void Write(BinaryWriter writer, Identity value) =>
                ByteArray.Instance.Write(writer, value.bytes);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Product(
                    // Special name recognised by STDB generator.
                    [new("__identity_bytes", ByteArray.Instance.GetAlgebraicType(registrar))]
                );
        }
    }

    public readonly struct Address(byte[] bytes) : IEquatable<Address>
    {
        private readonly byte[] bytes = bytes;
        public static readonly Address Zero = new(new byte[16]);

        public bool Equals(Address other) => bytes.SequenceEqual(other.bytes);

        public override bool Equals(object? obj) => obj is Address other && Equals(other);

        public static bool operator ==(Address left, Address right) => left.Equals(right);

        public static bool operator !=(Address left, Address right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        // We need to implement this manually because `spacetime generate` only
        // recognises inline product type with special field name and
        // not types registered on ModuleDef (which is what [SpacetimeDB.Type] does).
        public readonly struct BSATN : IReadWrite<Address>
        {
            public Address Read(BinaryReader reader) => new(ByteArray.Instance.Read(reader));

            public void Write(BinaryWriter writer, Address value) =>
                ByteArray.Instance.Write(writer, value.bytes);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Product(
                    // Special name recognised by STDB generator.
                    [new("__address_bytes", ByteArray.Instance.GetAlgebraicType(registrar))]
                );
        }
    }

    public class ReducerContext
    {
        public readonly Identity Sender;
        public readonly DateTimeOffset Time;
        public readonly Address? Address;

        public ReducerContext(byte[] senderIdentity, byte[] senderAddress, ulong timestamp_us)
        {
            Sender = new Identity(senderIdentity);
            var addr = new Address(senderAddress);
            Address = addr == Runtime.Address.Zero ? null : addr;
            // timestamp is in microseconds; the easiest way to convert those w/o losing precision is to get Unix origin and add ticks which are 0.1ms each.
            Time = DateTimeOffset.UnixEpoch.AddTicks(10 * (long)timestamp_us);
        }
    }

    public class ScheduleToken
    {
        private readonly RawBindings.ScheduleToken handle;

        public ScheduleToken(string name, byte[] args, DateTimeOffset time)
        {
            var name_bytes = UTF8.GetBytes(name);

            RawBindings._schedule_reducer(
                name_bytes,
                (uint)name_bytes.Length,
                args,
                (uint)args.Length,
                (ulong)((time - DateTimeOffset.UnixEpoch).Ticks / 10),
                out handle
            );
        }

        public void Cancel() => RawBindings._cancel_reducer(handle);
    }

    public static RawBindings.TableId GetTableId(string name)
    {
        var name_bytes = UTF8.GetBytes(name);
        RawBindings._get_table_id(name_bytes, (uint)name_bytes.Length, out var out_);
        return out_;
    }

    public static byte[] Insert<T>(RawBindings.TableId tableId, T row)
        where T : IStructuralReadWrite
    {
        var bytes = IStructuralReadWrite.ToBytes(row);
        RawBindings._insert(tableId, bytes, (uint)bytes.Length);
        return bytes;
    }

    public static uint DeleteByColEq(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value
    )
    {
        RawBindings._delete_by_col_eq(tableId, colId, value, (uint)value.Length, out var out_);
        return out_;
    }

    public static bool UpdateByColEq<T>(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value,
        T row
    )
        where T : IStructuralReadWrite
    {
        // Just like in Rust bindings, updating is just deleting and inserting for now.
        if (DeleteByColEq(tableId, colId, value) > 0)
        {
            Insert(tableId, row);
            return true;
        }
        else
        {
            return false;
        }
    }

    // An instance of `System.Random` that is reseeded by each reducer's timestamp.
    public static Random Random { get; internal set; } = new();
}
