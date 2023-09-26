namespace SpacetimeDB;

using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

public static class Runtime
{
    private static void ThrowForResult(ushort result)
    {
        throw new Exception($"SpacetimeDB error code: {result}");
    }

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern uint CreateTable(string name, byte[] schema);

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern uint GetTableId(string name);

    [SpacetimeDB.Type]
    public enum IndexType : byte
    {
        BTree,
        Hash,
    }

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern void CreateIndex(
        string index_name,
        uint table_id,
        IndexType index_type,
        byte[] col_ids
    );

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern byte[] IterByColEq(uint table_id, uint col_id, byte[] value);

    [MethodImpl(MethodImplOptions.InternalCall)]
    public extern static void Insert(uint tableId, byte[] row);

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern void DeletePk(uint table_id, byte[] pk);

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern void DeleteValue(uint table_id, byte[] row);

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern uint DeleteByColEq(uint tableId, uint colId, byte[] value);

    public static bool UpdateByColEq(uint tableId, uint colId, byte[] value, byte[] row)
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

    [MethodImpl(MethodImplOptions.InternalCall)]
    public static extern uint DeleteRange(
        uint tableId,
        uint colId,
        byte[] rangeStart,
        byte[] rangeEnd
    );

    // Ideally these methods would be scoped under BufferIter,
    // but Mono bindings don't seem to work correctly with nested
    // classes.

    [MethodImpl(MethodImplOptions.InternalCall)]
    private static extern void BufferIterStart(uint table_id, out uint handle);

    [MethodImpl(MethodImplOptions.InternalCall)]
    private static extern void BufferIterStartFiltered(
        uint table_id,
        byte[] filter,
        out uint handle
    );

    [MethodImpl(MethodImplOptions.InternalCall)]
    private static extern byte[]? BufferIterNext(uint handle);

    [MethodImpl(MethodImplOptions.InternalCall)]
    private extern static void BufferIterDrop(ref uint handle);

    private class BufferIter : IEnumerator<byte[]>, IDisposable
    {
        private uint handle;
        public byte[] Current { get; private set; } = new byte[0];

        object IEnumerator.Current => Current;

        public BufferIter(uint table_id, byte[]? filterBytes)
        {
            if (filterBytes is not null)
            {
                BufferIterStartFiltered(table_id, filterBytes, out handle);
            }
            else
            {
                BufferIterStart(table_id, out handle);
            }
        }

        public bool MoveNext()
        {
            Current = new byte[0];
            var next = BufferIterNext(handle);
            if (next is not null)
            {
                Current = next;
            }
            return next is not null;
        }

        public void Dispose()
        {
            BufferIterDrop(ref handle);
        }

        // Free unmanaged resource just in case user hasn't disposed for some reason.
        ~BufferIter()
        {
            // we already guard against double-free in stdb_iter_drop.
            Dispose();
        }

        public void Reset()
        {
            throw new NotImplementedException();
        }
    }

    public class RawTableIter : IEnumerable<byte[]>
    {
        public readonly byte[] Schema;

        private readonly IEnumerator<byte[]> iter;

        public RawTableIter(uint tableId, byte[]? filterBytes = null)
        {
            iter = new BufferIter(tableId, filterBytes);
            iter.MoveNext();
            Schema = iter.Current;
        }

        public IEnumerator<byte[]> GetEnumerator()
        {
            return iter;
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

    [MethodImpl(MethodImplOptions.InternalCall)]
    public extern static void Log(
        string text,
        LogLevel level = LogLevel.Info,
        [CallerMemberName] string target = "",
        [CallerFilePath] string filename = "",
        [CallerLineNumber] uint lineNumber = 0
    );

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

    public class DbEventArgs : EventArgs
    {
        public readonly Identity Sender;
        public readonly DateTimeOffset Time;

        public DbEventArgs(byte[] senderIdentity, ulong timestamp_us)
        {
            Sender = new Identity(senderIdentity);
            // timestamp is in microseconds; the easiest way to convert those w/o losing precision is to get Unix origin and add ticks which are 0.1ms each.
            Time = DateTimeOffset.UnixEpoch.AddTicks(10 * (long)timestamp_us);
        }
    }

    public static event Action<DbEventArgs>? OnConnect;
    public static event Action<DbEventArgs>? OnDisconnect;

    // Note: this is accessed by C bindings.
    private static string? IdentityConnected(byte[] sender_identity, ulong timestamp)
    {
        try
        {
            OnConnect?.Invoke(new(sender_identity, timestamp));
            return null;
        }
        catch (Exception e)
        {
            return e.ToString();
        }
    }

    // Note: this is accessed by C bindings.
    private static string? IdentityDisconnected(byte[] sender_identity, ulong timestamp)
    {
        try
        {
            OnDisconnect?.Invoke(new(sender_identity, timestamp));
            return null;
        }
        catch (Exception e)
        {
            return e.ToString();
        }
    }

    [MethodImpl(MethodImplOptions.InternalCall)]
    private extern static void ScheduleReducer(
        string name,
        byte[] args,
        // by-value ulong + other args corrupts stack in Mono's FFI for some reason
        // pass by reference (`in`) instead
        in ulong time,
        out ulong handle
    );

    [MethodImpl(MethodImplOptions.InternalCall)]
    private extern static void CancelReducer(
        // see ScheduleReducer for why we're using reference here
        in ulong handle
    );

    public class ScheduleToken
    {
        private readonly ulong handle;

        public ScheduleToken(string name, byte[] args, DateTimeOffset time) =>
            ScheduleReducer(
                name,
                args,
                (ulong)((time - DateTimeOffset.UnixEpoch).Ticks / 10),
                out handle
            );

        public void Cancel() => CancelReducer(handle);
    }
}
