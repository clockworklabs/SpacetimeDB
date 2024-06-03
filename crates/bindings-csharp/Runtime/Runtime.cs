namespace SpacetimeDB;

using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using SpacetimeDB.SATS;
using static System.Text.Encoding;

public static class Runtime
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

    public struct Identity(byte[] bytes) : IEquatable<Identity>
    {
        private readonly byte[] bytes = bytes;

        public bool Equals(Identity other) =>
            StructuralComparisons.StructuralEqualityComparer.Equals(bytes, other.bytes);

        public override bool Equals(object? obj) => obj is Identity other && Equals(other);

        public static bool operator ==(Identity left, Identity right) => left.Equals(right);

        public static bool operator !=(Identity left, Identity right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        private static SpacetimeDB.SATS.TypeInfo<Identity> satsTypeInfo =
            new(
                // We need to set type info to inlined identity type as `generate` CLI currently can't recognise type references for built-ins.
                new SpacetimeDB.SATS.AlgebraicType.Product(
                    [
                        new(
                            "__identity_bytes",
                            SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.AlgebraicType
                        )
                    ]
                ),
                reader => new(SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Read(reader)),
                (writer, value) =>
                    SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Write(writer, value.bytes)
            );

        public static SpacetimeDB.SATS.TypeInfo<Identity> GetSatsTypeInfo() => satsTypeInfo;
    }

    public struct Address(byte[] bytes) : IEquatable<Address>
    {
        private readonly byte[] bytes = bytes;
        public static readonly Address Zero = new(new byte[16]);

        public bool Equals(Address other) =>
            StructuralComparisons.StructuralEqualityComparer.Equals(bytes, other.bytes);

        public override bool Equals(object? obj) => obj is Address other && Equals(other);

        public static bool operator ==(Address left, Address right) => left.Equals(right);

        public static bool operator !=(Address left, Address right) => !left.Equals(right);

        public override int GetHashCode() =>
            StructuralComparisons.StructuralEqualityComparer.GetHashCode(bytes);

        public override string ToString() => BitConverter.ToString(bytes);

        private static SpacetimeDB.SATS.TypeInfo<Address> satsTypeInfo =
            new(
                // We need to set type info to inlined address type as `generate` CLI currently can't recognise type references for built-ins.
                new SpacetimeDB.SATS.AlgebraicType.Product(
                    [
                        new(
                            "__address_bytes",
                            SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.AlgebraicType
                        )
                    ]
                ),
                // Concern: We use this "packed" representation (as Bytes)
                //          in the caller_id field of reducer arguments,
                //          but in table rows,
                //          we send the "unpacked" representation as a product value.
                //          It's possible that these happen to be identical
                //          because BSATN is minimally self-describing,
                //          but that doesn't seem like something we should count on.
                reader => new(SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Read(reader)),
                (writer, value) =>
                    SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Write(writer, value.bytes)
            );

        public static SpacetimeDB.SATS.TypeInfo<Address> GetSatsTypeInfo() => satsTypeInfo;
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

    public static void Insert(RawBindings.TableId tableId, byte[] row)
    {
        RawBindings._insert(tableId, row, (uint)row.Length);
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

    public static bool UpdateByColEq(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value,
        byte[] row
    )
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

    public class RawTableIter(RawBindings.TableId tableId) : IEnumerable<byte[]>
    {
        public IEnumerator<byte[]> GetEnumerator()
        {
            RawBindings._iter_start(tableId, out var handle);
            return new RowIter(handle);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }
    }

    public class RawTableIterFiltered(RawBindings.TableId tableId, byte[] filterBytes) : IEnumerable<byte[]>
    {
        public IEnumerator<byte[]> GetEnumerator()
        {
            RawBindings._iter_start_filtered(
                tableId,
                filterBytes,
                (uint)filterBytes.Length,
                out var handle
            );
            return new RowIter(handle);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }
    }


    public class RawTableIterByColEq(RawBindings.TableId tableId, RawBindings.ColId colId, byte[] value) : IEnumerable<byte[]>
    {
        public IEnumerator<byte[]> GetEnumerator()
        {
            RawBindings._iter_by_col_eq(tableId, colId, value, (uint)value.Length, out var handle);
            return new RowIter(handle);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }
    }
}
