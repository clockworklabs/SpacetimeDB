namespace SpacetimeDB.Testing;

using System.Runtime.InteropServices;
using System.Text;

internal enum NativeStatus : uint
{
    Ok = 0,
    InvalidArgument = 1,
    InvalidSchema = 2,
    DatabaseError = 3,
    InternalError = 4,
    TransactionFinished = 5,
    TransactionActive = 6,
    DatastoreClosed = 7,
    UniqueViolation = 8,
    AutoIncOverflow = 9,
    NotSelect = 10,
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeBuffer : IDisposable
{
    internal byte* Data;
    internal ulong Length;

    internal readonly ReadOnlySpan<byte> Bytes => new(Data, checked((int)Length));

    public void Dispose() => Native.BufferFree(ref this);
}

internal static unsafe class Native
{
    private const string Library = "spacetimedb_test_datastore_ffi";
    private const uint AbiVersion = 1;

    internal static void EnsureAvailable()
    {
        try
        {
            ValidateVersion(GetAbiVersion());
        }
        catch (DllNotFoundException e)
        {
            throw new DllNotFoundException(
                $"Cannot load {Library} for {RuntimeInformation.RuntimeIdentifier}. "
                    + "Ensure the matching SpacetimeDB.Testing native library is in the test output "
                    + "and its system dependencies are installed.",
                e
            );
        }
        catch (EntryPointNotFoundException e)
        {
            throw new InvalidOperationException(
                "The native testing library has no ABI version export. Restore matching SpacetimeDB.Testing managed and native assets.",
                e
            );
        }
        catch (BadImageFormatException e)
        {
            throw new BadImageFormatException(
                $"The native testing library is incompatible with {RuntimeInformation.RuntimeIdentifier}. Restore native assets matching the test process architecture.",
                e
            );
        }
    }

    internal static void ValidateVersion(uint version)
    {
        if (version != AbiVersion)
            throw new InvalidOperationException(
                $"Native testing ABI mismatch: expected {AbiVersion}, found {version}. Restore matching SpacetimeDB.Testing managed and native assets."
            );
    }

    internal static void Check(NativeStatus status, ref NativeBuffer error)
    {
        try
        {
            if (status == NativeStatus.Ok)
                return;
            var message = Encoding.UTF8.GetString(error.Bytes);
            throw status switch
            {
                NativeStatus.InvalidArgument or NativeStatus.InvalidSchema => new ArgumentException(
                    message
                ),
                NativeStatus.DatastoreClosed => new ObjectDisposedException(
                    "NativeDatastore",
                    message
                ),
                NativeStatus.UniqueViolation => new UniqueConstraintViolationException(),
                NativeStatus.AutoIncOverflow => new AutoIncOverflowException(),
                _ => new InvalidOperationException($"{status}: {message}"),
            };
        }
        finally
        {
            error.Dispose();
        }
    }

    internal static byte[] TakeBytes(
        NativeStatus status,
        ref NativeBuffer output,
        ref NativeBuffer error
    )
    {
        try
        {
            Check(status, ref error);
            return output.Bytes.ToArray();
        }
        finally
        {
            output.Dispose();
        }
    }

    [DllImport(
        Library,
        EntryPoint = "stdb_test_abi_version",
        CallingConvention = CallingConvention.Cdecl
    )]
    private static extern uint GetAbiVersion();

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_buffer_free",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus BufferFree(ref NativeBuffer buffer);

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_datastore_create",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Create(
        byte* schema,
        ulong length,
        out nint datastore,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_datastore_destroy",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Destroy(nint datastore);

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_transaction_begin",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Begin(
        NativeDatastore datastore,
        out nint transaction,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_transaction_destroy",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus DestroyTransaction(nint transaction);

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_transaction_commit",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Commit(
        NativeTransaction transaction,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_transaction_rollback",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Rollback(
        NativeTransaction transaction,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_table_id",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus TableId(
        NativeTransaction transaction,
        byte* name,
        ulong length,
        out uint id,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_table_count",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Count(
        NativeTransaction transaction,
        uint table,
        out ulong count,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_table_insert",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Insert(
        NativeTransaction transaction,
        uint table,
        byte* row,
        ulong length,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [DllImport(
        Library,
        EntryPoint = "stdb_test_v1_table_scan",
        CallingConvention = CallingConvention.Cdecl
    )]
    internal static extern NativeStatus Scan(
        NativeTransaction transaction,
        uint table,
        out NativeBuffer output,
        out NativeBuffer error
    );
}
