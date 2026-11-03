namespace SpacetimeDB.Testing;

using System.Text;
using Microsoft.Win32.SafeHandles;
using SpacetimeDB.BSATN;

internal sealed unsafe class NativeDatastore : SafeHandleZeroOrMinusOneIsInvalid
{
    // Shared with transaction handles, including their finalizers. The native ABI
    // requires serialization even when a datastore is disposed before its transaction.
    internal object Sync { get; } = new();

    internal NativeDatastore()
        : base(ownsHandle: true) { }

    internal static NativeDatastore Create(ReadOnlySpan<byte> schema)
    {
        Native.EnsureAvailable();
        var datastore = new NativeDatastore();
        try
        {
            fixed (byte* bytes = schema)
            {
                var status = Native.Create(
                    bytes,
                    (ulong)schema.Length,
                    out var handle,
                    out var error
                );
                datastore.SetHandle(handle);
                Native.Check(status, ref error);
            }
            return datastore;
        }
        catch
        {
            datastore.Dispose();
            throw;
        }
    }

    internal NativeTransaction Begin()
    {
        lock (Sync)
        {
            var transaction = new NativeTransaction(this);
            try
            {
                var status = Native.Begin(this, out var handle, out var error);
                transaction.Initialize(handle);
                Native.Check(status, ref error);
                return transaction;
            }
            catch
            {
                transaction.Dispose();
                throw;
            }
        }
    }

    internal byte[] Query(string sql, Identity identity)
    {
        var sqlBytes = Encoding.UTF8.GetBytes(sql);
        var identityBytes = IStructuralReadWrite.ToBytes(new Identity.BSATN(), identity);
        lock (Sync)
        {
            fixed (
                byte* sqlPtr = sqlBytes,
                    identityPtr = identityBytes
            )
            {
                var status = Native.Query(
                    this,
                    sqlPtr,
                    (ulong)sqlBytes.Length,
                    identityPtr,
                    (ulong)identityBytes.Length,
                    out var output,
                    out var error
                );
                return Native.TakeBytes(status, ref output, ref error);
            }
        }
    }

    protected override bool ReleaseHandle()
    {
        lock (Sync)
            return Native.Destroy(handle) == NativeStatus.Ok;
    }
}
