namespace SpacetimeDB;

using System;
using System.Collections;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;
using static System.Text.Encoding;

public static partial class RawBindings
{
    // For now this must match the name of the `.c` file (`bindings.c`).
    // In the future C# will allow to specify Wasm import namespace in
    // `LibraryImport` directly.
    const string StdbNamespace = "bindings";

    // This custom marshaller takes care of checking the status code
    // returned from the host and throwing an exception if it's not 0.
    // The only reason it doesn't return `void` is because the C# compiler
    // doesn't treat `void` as a real type and doesn't allow it to be returned
    // from custom marshallers, so we resort to an empty struct instead.
    [CustomMarshaller(
        typeof(CheckedStatus),
        MarshalMode.ManagedToUnmanagedOut,
        typeof(StatusMarshaller)
    )]
    static class StatusMarshaller
    {
        public static CheckedStatus ConvertToManaged(ushort status)
        {
            if (status != 0)
            {
                throw new Exception(
                    status switch
                    {
                        1 => "No such table",
                        2 => "Value or range provided not found in table",
                        3 => "Value with given unique identifier already exists",
                        _ => $"SpacetimeDB error code {status}",
                    }
                );
            }
            return default;
        }
    }

    [NativeMarshalling(typeof(StatusMarshaller))]
    public struct CheckedStatus;

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct TableId
    {
        private readonly uint table_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct ColId(uint col_id)
    {
        private readonly uint col_id = col_id;

        public static explicit operator uint(ColId col_id) => col_id.col_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct IndexType
    {
        private readonly byte index_type;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct LogLevel(byte log_level)
    {
        private readonly byte log_level = log_level;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct ScheduleToken
    {
        private readonly ulong schedule_token;
    }

    // We need custom marshaller for `Buffer` because we return it by value
    // instead of passing an `out` reference, and C# currently doesn't match
    // the common Wasm C ABI in that a struct with a single field is supposed
    // to have the same ABI as the field itself.
    [CustomMarshaller(typeof(Buffer), MarshalMode.Default, typeof(BufferMarshaller))]
    static class BufferMarshaller
    {
        public static Buffer ConvertToManaged(uint buf_handle) => new(buf_handle);

        public static uint ConvertToUnmanaged(Buffer buf) => (uint)buf;
    }

    [StructLayout(LayoutKind.Sequential)]
    [NativeMarshalling(typeof(BufferMarshaller))]
    public readonly struct Buffer(uint handle) : IEquatable<Buffer>
    {
        private readonly uint handle = handle;
        public static readonly Buffer INVALID = new(uint.MaxValue);

        public bool Equals(Buffer other) => handle == other.handle;

        public static explicit operator uint(Buffer buf) => buf.handle;

        public override bool Equals(object? obj) => obj is Buffer other && Equals(other);

        public override int GetHashCode() => handle.GetHashCode();

        public static bool operator ==(Buffer left, Buffer right) => left.Equals(right);

        public static bool operator !=(Buffer left, Buffer right) => !(left == right);
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct BufferIter(uint handle) : IEquatable<BufferIter>
    {
        private readonly uint handle = handle;
        public static readonly BufferIter INVALID = new(uint.MaxValue);

        public bool Equals(BufferIter other) => handle == other.handle;

        public static explicit operator uint(BufferIter buf) => buf.handle;

        public override bool Equals(object? obj) => obj is BufferIter other && Equals(other);

        public override int GetHashCode() => handle.GetHashCode();

        public static bool operator ==(BufferIter left, BufferIter right) => left.Equals(right);

        public static bool operator !=(BufferIter left, BufferIter right) => !(left == right);
    }

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _get_table_id(
        [In] byte[] name,
        uint name_len,
        out TableId out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _create_index(
        [In] byte[] index_name,
        uint index_name_len,
        TableId table_id,
        IndexType index_type,
        [In] ColId[] col_ids,
        uint col_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_by_col_eq(
        TableId table_id,
        ColId col_id,
        [In] byte[] value,
        uint value_len,
        out Buffer out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _insert(TableId table_id, byte[] row, uint row_len);

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _delete_by_col_eq(
        TableId table_id,
        ColId col_id,
        [In] byte[] value,
        uint value_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _delete_by_rel(
        TableId table_id,
        [In] byte[] relation,
        uint relation_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_start(TableId table_id, out BufferIter out_);

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_start_filtered(
        TableId table_id,
        [In] byte[] filter,
        uint filter_len,
        out BufferIter out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_next(BufferIter iter_handle, out Buffer out_);

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_drop(BufferIter iter_handle);

    [LibraryImport(StdbNamespace)]
    public static partial void _console_log(
        byte level,
        [In] byte[] target,
        uint target_len,
        [In] byte[] filename,
        uint filename_len,
        uint line_number,
        [In] byte[] message,
        uint message_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial void _schedule_reducer(
        [In] byte[] name,
        uint name_len,
        [In] byte[] args,
        uint args_len,
        ulong time,
        out ScheduleToken out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial void _cancel_reducer(ScheduleToken schedule_token_handle);

    [LibraryImport(StdbNamespace)]
    public static partial uint _buffer_len(Buffer buf_handle);

    [LibraryImport(StdbNamespace)]
    public static partial void _buffer_consume(
        Buffer buf_handle,
        [MarshalUsing(CountElementName = nameof(dst_len))] [Out] byte[] dst,
        uint dst_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial Buffer _buffer_alloc([In] byte[] data, uint data_len);
}
