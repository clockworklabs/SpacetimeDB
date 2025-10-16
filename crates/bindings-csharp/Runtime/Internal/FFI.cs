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
    NO_SUCH_INDEX = 5,
    NO_SUCH_ITER = 6,
    NO_SUCH_CONSOLE_TIMER = 7,
    NO_SUCH_BYTES = 8,
    NO_SPACE = 9,
    BUFFER_TOO_SMALL = 11,
    UNIQUE_ALREADY_EXISTS = 12,
    SCHEDULE_AT_DELAY_TOO_LONG = 13,
    INDEX_NOT_UNIQUE = 14,
    NO_SUCH_ROW = 15,
    AUTO_INC_OVERFLOW = 16,
}

#pragma warning disable IDE1006 // Naming Styles - Not applicable to FFI stuff.
internal static partial class FFI
{
    // For now this must match the name of the `.c` file (`bindings.c`).
    // In the future C# will allow to specify Wasm import namespace in
    // `LibraryImport` directly.
    const string StdbNamespace10_0 =
#if EXPERIMENTAL_WASM_AOT
        "spacetime_10.0"
#else
        "bindings"
#endif
    ;

    const string StdbNamespace10_1 =
#if EXPERIMENTAL_WASM_AOT
        "spacetime_10.1"
#else
        "bindings"
#endif
    ;

    const string StdbNamespace10_2 =
#if EXPERIMENTAL_WASM_AOT
        "spacetime_10.2"
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
                    Errno.NO_SUCH_INDEX => new NoSuchIndexException(),
                    Errno.NO_SUCH_ITER => new NoSuchIterException(),
                    Errno.NO_SUCH_CONSOLE_TIMER => new NoSuchLogStopwatch(),
                    Errno.NO_SUCH_BYTES => new NoSuchBytesException(),
                    Errno.NO_SPACE => new NoSpaceException(),
                    Errno.BUFFER_TOO_SMALL => new BufferTooSmallException(),
                    Errno.UNIQUE_ALREADY_EXISTS => new UniqueConstraintViolationException(),
                    Errno.SCHEDULE_AT_DELAY_TOO_LONG => new ScheduleAtDelayTooLongException(),
                    Errno.INDEX_NOT_UNIQUE => new IndexNotUniqueException(),
                    Errno.NO_SUCH_ROW => new NoSuchRowException(),
                    Errno.AUTO_INC_OVERFLOW => new AutoIncOverflowException(),
                    _ => new UnknownException(status),
                };
            }
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct TableId
    {
        private readonly uint table_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct IndexId
    {
        private readonly uint index_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct ColId(ushort col_id)
    {
        private readonly ushort col_id = col_id;

        public static explicit operator ushort(ColId col_id) => col_id.col_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly struct IndexType
    {
        private readonly byte index_type;
    }

    [StructLayout(LayoutKind.Sequential)]
    public readonly record struct RowIter(uint Handle)
    {
        public static readonly RowIter INVALID = new(0);
    }

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus table_id_from_name(
        [In] byte[] name,
        uint name_len,
        out TableId out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus index_id_from_name(
        [In] byte[] name,
        uint name_len,
        out IndexId out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_table_row_count(TableId table_id, out ulong out_);

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_table_scan_bsatn(
        TableId table_id,
        out RowIter out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_index_scan_range_bsatn(
        IndexId index_id,
        ReadOnlySpan<byte> prefix,
        uint prefix_len,
        ColId prefix_elems,
        ReadOnlySpan<byte> rstart,
        uint rstart_len,
        ReadOnlySpan<byte> rend,
        uint rend_len,
        out RowIter out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial Errno row_iter_bsatn_advance(
        RowIter iter_handle,
        [MarshalUsing(CountElementName = nameof(buffer_len))] [Out] byte[] buffer,
        ref uint buffer_len
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus row_iter_bsatn_close(RowIter iter_handle);

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_insert_bsatn(
        TableId table_id,
        Span<byte> row,
        ref uint row_len
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_update_bsatn(
        TableId table_id,
        IndexId index_id,
        Span<byte> row,
        ref uint row_len
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_delete_by_index_scan_range_bsatn(
        IndexId index_id,
        ReadOnlySpan<byte> prefix,
        uint prefix_len,
        ColId prefix_elems,
        ReadOnlySpan<byte> rstart,
        uint rstart_len,
        ReadOnlySpan<byte> rend,
        uint rend_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus datastore_delete_all_by_eq_bsatn(
        TableId table_id,
        [In] byte[] relation,
        uint relation_len,
        out uint out_
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial Errno bytes_source_read(
        BytesSource source,
        Span<byte> buffer,
        ref uint buffer_len
    );

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus bytes_sink_write(
        BytesSink sink,
        ReadOnlySpan<byte> buffer,
        ref uint buffer_len
    );

    public enum LogLevel : byte
    {
        Error = 0,
        Warn = 1,
        Info = 2,
        Debug = 3,
        Trace = 4,
        Panic = 5,
    }

    [LibraryImport(StdbNamespace10_0)]
    public static partial void console_log(
        LogLevel level,
        [In] byte[] target,
        uint target_len,
        [In] byte[] filename,
        uint filename_len,
        uint line_number,
        [In] byte[] message,
        uint message_len
    );

    [NativeMarshalling(typeof(ConsoleTimerIdMarshaller))]
    [StructLayout(LayoutKind.Sequential)]
    public readonly struct ConsoleTimerId
    {
        private readonly uint timer_id;

        private ConsoleTimerId(uint id)
        {
            timer_id = id;
        }

        //LayoutKind.Sequential is apparently not enough for this struct to be returnable in PInvoke, so we need a custom marshaller unfortunately
        [CustomMarshaller(
            typeof(ConsoleTimerId),
            MarshalMode.Default,
            typeof(ConsoleTimerIdMarshaller)
        )]
        internal static class ConsoleTimerIdMarshaller
        {
            public static ConsoleTimerId ConvertToManaged(uint id) => new ConsoleTimerId(id);

            public static uint ConvertToUnmanaged(ConsoleTimerId id) => id.timer_id;
        }
    }

    [LibraryImport(StdbNamespace10_0)]
    public static partial ConsoleTimerId console_timer_start([In] byte[] name, uint name_len);

    [LibraryImport(StdbNamespace10_0)]
    public static partial CheckedStatus console_timer_end(ConsoleTimerId stopwatch_id);

    [LibraryImport(StdbNamespace10_0)]
    public static partial void volatile_nonatomic_schedule_immediate(
        [In] byte[] name,
        uint name_len,
        [In] byte[] args,
        uint args_len
    );

    // Note #1: our Identity type has the same layout as a fixed-size 32-byte little-endian buffer,
    // so instead of working around C#'s lack of fixed-size arrays, we just accept the pointer to
    // the Identity itself. In this regard it's different from Rust declaration, but is still
    // functionally the same.
    // Note #2: we can't use `LibraryImport` here due to https://github.com/dotnet/runtime/issues/98616
    // which prevents source-generated PInvokes from working with types from other assemblies, and
    // `Identity` lives in another assembly (`BSATN.Runtime`). Luckily, `DllImport` is enough here.
#pragma warning disable SYSLIB1054 // Suppress "Use 'LibraryImportAttribute' instead of 'DllImportAttribute'" warning.
    [DllImport(StdbNamespace10_0)]
    public static extern void identity(out Identity dest);
#pragma warning restore SYSLIB1054

    [DllImport(StdbNamespace10_1)]
    public static extern Errno bytes_source_remaining_length(BytesSource source, ref uint len);

    [DllImport(StdbNamespace10_2)]
    public static extern Errno get_jwt(ref ConnectionId connectionId, out BytesSource source);
}
