namespace SpacetimeDB.Internal;

using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;

// This type is outside of the hidden `FFI` class because for now we need to do some public
// forwarding in the codegen for `__describe_module__` and `__call_reducer__` exports which both
// use this type.
[StructLayout(LayoutKind.Sequential)]
public readonly record struct BytesSource(uint Handle)
{
    public static readonly BytesSource INVALID = new(0);
}

// This type is outside of the hidden `FFI` class because for now we need to do some public
// forwarding in the codegen for `__describe_module__` and `__call_reducer__` exports which both
// use this type.
[StructLayout(LayoutKind.Sequential)]
public readonly record struct BytesSink(uint Handle) { }

public enum Errno : short
{
    EXHAUSTED = -1,
    OK = 0,
    HOST_CALL_FAILURE = 1,
    NOT_IN_TRANSACTION = 2,
    BSATN_DECODE_ERROR = 3,
    NO_SUCH_TABLE = 4,
    NO_SUCH_ITER = 6,
    NO_SUCH_BYTES = 8,
    NO_SPACE = 9,
    BUFFER_TOO_SMALL = 11,
    UNIQUE_ALREADY_EXISTS = 12,
    SCHEDULE_AT_DELAY_TOO_LONG = 13,
}

[StructLayout(LayoutKind.Sequential)]
public readonly struct TableId {
    private readonly uint table_id;
}

[StructLayout(LayoutKind.Sequential)]
public readonly struct ColId(ushort col_id) {
    private readonly ushort col_id = col_id;

    public static explicit operator ushort(ColId col_id) => col_id.col_id;
}

[StructLayout(LayoutKind.Sequential)]
public readonly struct IndexType {
    private readonly byte index_type;
}

[StructLayout(LayoutKind.Sequential)]
public readonly struct LogLevel(byte log_level) {
    private readonly byte log_level = log_level;
}

[StructLayout(LayoutKind.Sequential)]
public readonly record struct RowIter(uint Handle) {
    public static readonly RowIter INVALID = new(0);
}

#pragma warning disable IDE1006 // Naming Styles - Not applicable to FFI stuff.
internal static partial class FFI
{
    // For now this must match the name of the `.c` file (`bindings.c`).
    // In the future C# will allow to specify Wasm import namespace in
    // `LibraryImport` directly.
    const string StdbNamespace =
#if EXPERIMENTAL_WASM_AOT
        "spacetime_10.0"
#else
        "bindings"
#endif
    ;

    [NativeMarshalling(typeof(Marshaller))]
    public struct CheckedStatus
    {
        // This custom marshaller takes care of checking the status code
        // returned from the host and throwing an exception if it's not 0.
        // The only reason it doesn't return `void` is because the C# compiler
        // doesn't treat `void` as a real type and doesn't allow it to be returned
        // from custom marshallers, so we resort to an empty struct instead.
        [CustomMarshaller(
            typeof(CheckedStatus),
            MarshalMode.ManagedToUnmanagedOut,
            typeof(Marshaller)
        )]
        internal static class Marshaller
        {
            public static CheckedStatus ConvertToManaged(Errno status)
            {
                if (status == 0)
                {
                    return default;
                }
                throw status switch
                {
                    Errno.NOT_IN_TRANSACTION => new NotInTransactionException(),
                    Errno.BSATN_DECODE_ERROR => new BsatnDecodeException(),
                    Errno.NO_SUCH_TABLE => new NoSuchTableException(),
                    Errno.NO_SUCH_ITER => new NoSuchIterException(),
                    Errno.NO_SUCH_BYTES => new NoSuchBytesException(),
                    Errno.NO_SPACE => new NoSpaceException(),
                    Errno.BUFFER_TOO_SMALL => new BufferTooSmallException(),
                    Errno.UNIQUE_ALREADY_EXISTS => new UniqueAlreadyExistsException(),
                    Errno.SCHEDULE_AT_DELAY_TOO_LONG => new ScheduleAtDelayTooLongException(),
                    _ => new UnknownException(status),
                };
            }
        }
    }

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _table_id_from_name(
        [In] byte[] name,
        uint name_len,
        out TableId out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _datastore_table_row_count(
        TableId table_id,
        out ulong out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _datastore_table_scan_bsatn(
        TableId table_id,
        out RowIter out_
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
    public static partial CheckedStatus _datastore_insert_bsatn(
        TableId table_id,
        Span<byte> row,
        ref uint row_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _delete_by_col_eq(
        TableId table_id,
        ColId col_id,
        [In] byte[] value,
        uint value_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _iter_start_filtered(
        TableId table_id,
        [In] byte[] filter,
        uint filter_len,
        out RowIter out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial Errno _row_iter_bsatn_advance(
        RowIter iter_handle,
        [MarshalUsing(CountElementName = nameof(buffer_len))] [Out] byte[] buffer,
        ref uint buffer_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _row_iter_bsatn_close(RowIter iter_handle);

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _datastore_delete_all_by_eq_bsatn(
        TableId table_id,
        [In] byte[] relation,
        uint relation_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace)]
    public static partial void _volatile_nonatomic_schedule_immediate(
        [In] byte[] name,
        uint name_len,
        [In] byte[] args,
        uint args_len
    );

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
    public static partial Errno _bytes_source_read(
        BytesSource source,
        Span<byte> buffer,
        ref uint buffer_len
    );

    [LibraryImport(StdbNamespace)]
    public static partial CheckedStatus _bytes_sink_write(
        BytesSink sink,
        ReadOnlySpan<byte> buffer,
        ref uint buffer_len
    );
}
