namespace SpacetimeDB.Testing;

using System.Text;
using Microsoft.Win32.SafeHandles;

// Retain the datastore while this handle is in use; explicit datastore disposal
// still closes outstanding transactions according to the native ABI.
internal sealed unsafe class NativeTransaction(NativeDatastore datastore)
    : SafeHandleZeroOrMinusOneIsInvalid(ownsHandle: true)
{
    internal void Initialize(nint value) => SetHandle(value);

    internal void Commit()
    {
        lock (datastore.Sync)
        {
            var status = Native.Commit(this, out var error);
            Native.Check(status, ref error);
        }
    }

    internal void Rollback()
    {
        lock (datastore.Sync)
        {
            var status = Native.Rollback(this, out var error);
            Native.Check(status, ref error);
        }
    }

    internal uint TableId(string name)
    {
        var utf8 = Encoding.UTF8.GetBytes(name);
        lock (datastore.Sync)
            fixed (byte* bytes = utf8)
            {
                var status = Native.TableId(
                    this,
                    bytes,
                    (ulong)utf8.Length,
                    out var id,
                    out var error
                );
                Native.Check(status, ref error);
                return id;
            }
    }

    internal ulong Count(uint table)
    {
        lock (datastore.Sync)
        {
            var status = Native.Count(this, table, out var count, out var error);
            Native.Check(status, ref error);
            return count;
        }
    }

    internal byte[] Insert(uint table, ReadOnlySpan<byte> row)
    {
        lock (datastore.Sync)
            fixed (byte* bytes = row)
            {
                var status = Native.Insert(
                    this,
                    table,
                    bytes,
                    (ulong)row.Length,
                    out var output,
                    out var error
                );
                return Native.TakeBytes(status, ref output, ref error);
            }
    }

    internal byte[] Scan(uint table)
    {
        lock (datastore.Sync)
        {
            var status = Native.Scan(this, table, out var output, out var error);
            return Native.TakeBytes(status, ref output, ref error);
        }
    }

    protected override bool ReleaseHandle()
    {
        lock (datastore.Sync)
            return Native.DestroyTransaction(handle) == NativeStatus.Ok;
    }
}
