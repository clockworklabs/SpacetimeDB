namespace SpacetimeDB;

using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
using static System.Text.Encoding;

public static class Runtime
{
    [SpacetimeDB.Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    private static byte[] Consume(this RawBindings.Buffer buffer)
    {
        var len = RawBindings._buffer_len(buffer);
        var result = new byte[len];
        RawBindings._buffer_consume(buffer, result, len);
        return result;
    }

    private class BufferIter : IEnumerator<byte[]>, IDisposable
    {
        private RawBindings.BufferIter handle;
        public byte[] Current { get; private set; } = new byte[0];

        object IEnumerator.Current => Current;

        public BufferIter(RawBindings.TableId table_id, byte[]? filterBytes)
        {
            if (filterBytes is null)
            {
                RawBindings._iter_start(table_id, out handle);
            }
            else
            {
                RawBindings._iter_start_filtered(
                    table_id,
                    filterBytes,
                    (uint)filterBytes.Length,
                    out handle
                );
            }
        }

        public bool MoveNext()
        {
            RawBindings._iter_next(handle, out var nextBuf);
            if (nextBuf.Equals(RawBindings.Buffer.INVALID))
            {
                return false;
            }
            Current = nextBuf.Consume();
            return true;
        }

        public void Dispose()
        {
            if (!handle.Equals(RawBindings.BufferIter.INVALID))
            {
                RawBindings._iter_drop(handle);
                handle = RawBindings.BufferIter.INVALID;
                // Avoid running ~BufferIter if Dispose was executed successfully.
                GC.SuppressFinalize(this);
            }
        }

        // Free unmanaged resource just in case user hasn't disposed for some reason.
        ~BufferIter()
        {
            // we already guard against double-free in Dispose.
            Dispose();
        }

        public void Reset()
        {
            throw new NotImplementedException();
        }
    }

    public class RawTableIter : IEnumerable<byte[]>
    {
        private readonly RawBindings.TableId tableId;
        private readonly byte[]? filterBytes;

        public RawTableIter(RawBindings.TableId tableId, byte[]? filterBytes = null)
        {
            this.tableId = tableId;
            this.filterBytes = filterBytes;
        }

        public IEnumerator<byte[]> GetEnumerator()
        {
            return new BufferIter(tableId, filterBytes);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
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

    public struct Identity : IEquatable<Identity>
    {
        private readonly byte[] bytes;

        public Identity(byte[] bytes) => this.bytes = bytes;

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
                new SpacetimeDB.SATS.ProductType
                {
                    { "__identity_bytes", SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.AlgebraicType }
                },
                reader => new(SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Read(reader)),
                (writer, value) =>
                    SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.Write(writer, value.bytes)
            );

        public static SpacetimeDB.SATS.TypeInfo<Identity> GetSatsTypeInfo() => satsTypeInfo;
    }

    public struct Address : IEquatable<Address>
    {
        private readonly byte[] bytes;

        public Address(byte[] bytes) => this.bytes = bytes;

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
                new SpacetimeDB.SATS.ProductType
                {
                    { "__address_bytes", SpacetimeDB.SATS.BuiltinType.BytesTypeInfo.AlgebraicType }
                },
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

    public class DbEventArgs : EventArgs
    {
        public readonly Identity Sender;
        public readonly DateTimeOffset Time;
        public readonly Address? Address;

        public DbEventArgs(byte[] senderIdentity, byte[] senderAddress, ulong timestamp_us)
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

    public static byte[] IterByColEq(
        RawBindings.TableId tableId,
        RawBindings.ColId colId,
        byte[] value
    )
    {
        RawBindings._iter_by_col_eq(tableId, colId, value, (uint)value.Length, out var buf);
        return buf.Consume();
    }
}
