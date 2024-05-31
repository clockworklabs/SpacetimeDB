namespace SpacetimeDB;

using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;

public static partial class RawBindings
{
    // For now this must match the name of the `.c` file (`bindings.c`).
    // In the future C# will allow to specify Wasm import namespace in
    // `LibraryImport` directly.
    const string StdbNamespace =
#if EXPERIMENTAL_WASM_AOT
        "spacetime_8.0"
#else
        "bindings"
#endif
    ;

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
                throw status switch
                {
                    ERRNO_NO_SUCH_TABLE => new NoSuchTableException(),
                    ERRNO_LOOKUP_NOT_FOUND => new LookupNotFoundException(),
                    ERRNO_UNIQUE_ALREADY_EXISTS => new UniqueAlreadyExistsException(),
                    ERRNO_BUFFER_TOO_SMALL => new BufferTooSmallException(),
                    _ => new StdbException(status),
                };
            }
            return default;
        }
    }

    private const ushort ERRNO_NO_SUCH_TABLE = 1;
    private const ushort ERRNO_LOOKUP_NOT_FOUND = 2;
    private const ushort ERRNO_UNIQUE_ALREADY_EXISTS = 3;
    private const ushort ERRNO_BUFFER_TOO_SMALL = 4;

    public class StdbException : Exception
    {
        public ushort Code { get; private set; }

        internal StdbException(ushort code) => Code = code;

        public override string Message => $"SpacetimeDB error code {Code}";
    }

    public class NoSuchTableException : StdbException
    {
        internal NoSuchTableException()
            : base(ERRNO_NO_SUCH_TABLE) { }

        public override string Message => "No such table";
    }

    public class LookupNotFoundException : StdbException
    {
        internal LookupNotFoundException()
            : base(ERRNO_LOOKUP_NOT_FOUND) { }

        public override string Message => "Value or range provided not found in table";
    }

    public class UniqueAlreadyExistsException : StdbException
    {
        internal UniqueAlreadyExistsException()
            : base(ERRNO_UNIQUE_ALREADY_EXISTS) { }

        public override string Message => "Value with given unique identifier already exists";
    }

    public class BufferTooSmallException : StdbException
    {
        internal BufferTooSmallException()
            : base(ERRNO_BUFFER_TOO_SMALL) { }

        public override string Message =>
            "The provided buffer is not large enough to store the data";
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

        public static uint ConvertToUnmanaged(Buffer buf) => buf.Handle;
    }

    [StructLayout(LayoutKind.Sequential)]
    [NativeMarshalling(typeof(BufferMarshaller))]
    public readonly record struct Buffer(uint Handle)
    {
        public static readonly Buffer INVALID = new(uint.MaxValue);
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly record struct RowIter(uint Handle)
    {
        public static readonly RowIter INVALID = new(uint.MaxValue);
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
        out RowIter out_
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
    public static partial CheckedStatus _iter_start(TableId table_id, out RowIter out_);

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_start_filtered(
        TableId table_id,
        [In] byte[] filter,
        uint filter_len,
        out RowIter out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_advance(
        RowIter iter_handle,
        // [MarshalUsing(CountElementName = nameof(dst_len))] [Out] byte[] dst,
        byte[] buffer,
        ref uint buffer_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial void _iter_drop(RowIter iter_handle);

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
