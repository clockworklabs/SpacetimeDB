namespace SpacetimeDB.Testing;

using System.Runtime.InteropServices;
using System.Text;

// Status codes returned by the native testing library. Keep in sync with the status code constants in `crates/test-datastore-ffi/src/lib.rs`.
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

internal static unsafe partial class Native
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

    internal static Identity IdentityFromClaims(string issuer, string subject)
    {
        EnsureAvailable();
        var issuerBytes = Encoding.UTF8.GetBytes(issuer);
        var subjectBytes = Encoding.UTF8.GetBytes(subject);
        fixed (
            byte* issuerPtr = issuerBytes,
                subjectPtr = subjectBytes
        )
        {
            var status = IdentityFromClaimsNative(
                issuerPtr,
                (ulong)issuerBytes.Length,
                subjectPtr,
                (ulong)subjectBytes.Length,
                out var output,
                out var error
            );
            return new Identity(TakeBytes(status, ref output, ref error));
        }
    }

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_identity_from_claims")]
    private static partial NativeStatus IdentityFromClaimsNative(
        byte* issuer,
        ulong issuerLength,
        byte* subject,
        ulong subjectLength,
        out NativeBuffer output,
        out NativeBuffer error
    );

    internal static void ValidateVersion(uint version)
    {
        if (version != AbiVersion)
        {
            throw new InvalidOperationException(
                $"Native testing ABI mismatch: expected {AbiVersion}, found {version}. Restore matching SpacetimeDB.Testing managed and native assets."
            );
        }
    }

    internal static void Check(NativeStatus status, ref NativeBuffer error)
    {
        try
        {
            if (status == NativeStatus.Ok)
            {
                return;
            }

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
            return [.. output.Bytes];
        }
        finally
        {
            output.Dispose();
        }
    }

    [LibraryImport(Library, EntryPoint = "stdb_test_abi_version")]
    private static partial uint GetAbiVersion();

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_buffer_free")]
    internal static partial NativeStatus BufferFree(ref NativeBuffer buffer);

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_datastore_create")]
    internal static partial NativeStatus Create(
        byte* schema,
        ulong length,
        out nint datastore,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_datastore_destroy")]
    internal static partial NativeStatus Destroy(nint datastore);

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_transaction_begin")]
    internal static partial NativeStatus Begin(
        NativeDatastore datastore,
        out nint transaction,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_query")]
    internal static partial NativeStatus Query(
        NativeDatastore datastore,
        byte* sql,
        ulong sqlLength,
        byte* identity,
        ulong identityLength,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_transaction_destroy")]
    internal static partial NativeStatus DestroyTransaction(nint transaction);

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_transaction_commit")]
    internal static partial NativeStatus Commit(
        NativeTransaction transaction,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_transaction_rollback")]
    internal static partial NativeStatus Rollback(
        NativeTransaction transaction,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_id")]
    internal static partial NativeStatus TableId(
        NativeTransaction transaction,
        byte* name,
        ulong length,
        out uint id,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_count")]
    internal static partial NativeStatus Count(
        NativeTransaction transaction,
        uint table,
        out ulong count,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_insert")]
    internal static partial NativeStatus Insert(
        NativeTransaction transaction,
        uint table,
        byte* row,
        ulong length,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_index_id")]
    internal static partial NativeStatus IndexId(
        NativeTransaction transaction,
        byte* name,
        ulong length,
        out uint id,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_index_scan_point")]
    internal static partial NativeStatus IndexScanPoint(
        NativeTransaction transaction,
        uint index,
        byte* point,
        ulong length,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_index_delete_point")]
    internal static partial NativeStatus IndexDeletePoint(
        NativeTransaction transaction,
        uint index,
        byte* point,
        ulong length,
        out uint deleted,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_index_scan_range")]
    internal static partial NativeStatus IndexScanRange(
        NativeTransaction transaction,
        uint index,
        byte* prefix,
        ulong prefixLength,
        ushort prefixElems,
        byte* start,
        ulong startLength,
        byte* end,
        ulong endLength,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_index_delete_range")]
    internal static partial NativeStatus IndexDeleteRange(
        NativeTransaction transaction,
        uint index,
        byte* prefix,
        ulong prefixLength,
        ushort prefixElems,
        byte* start,
        ulong startLength,
        byte* end,
        ulong endLength,
        out uint deleted,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_update")]
    internal static partial NativeStatus Update(
        NativeTransaction transaction,
        uint table,
        uint index,
        byte* row,
        ulong length,
        out NativeBuffer output,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_delete_row")]
    internal static partial NativeStatus DeleteRow(
        NativeTransaction transaction,
        uint table,
        byte* row,
        ulong length,
        out uint deleted,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_clear")]
    internal static partial NativeStatus Clear(
        NativeTransaction transaction,
        uint table,
        out ulong count,
        out NativeBuffer error
    );

    [LibraryImport(Library, EntryPoint = "stdb_test_v1_table_scan")]
    internal static partial NativeStatus Scan(
        NativeTransaction transaction,
        uint table,
        out NativeBuffer output,
        out NativeBuffer error
    );
}
