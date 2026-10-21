namespace SpacetimeDB.Testing.Tests;

using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.Loader;
using SpacetimeDB.BSATN;
using TestingModule;

public class InteropTests
{
    [Fact]
    public void GeneratedSchemaSupportsCommitRollbackAndDisposal()
    {
        using var datastore = NativeDatastore.Create(Schema.Build());
        ulong id;
        using (var transaction = datastore.Begin())
        {
            var table = transaction.TableId("entries");
            using var generated = Reader(transaction.Insert(table, Encode(0, "hello")));
            id = generated.ReadUInt64();
            Assert.NotEqual(0UL, id);
            Assert.Equal(generated.BaseStream.Length, generated.BaseStream.Position);
            Assert.Throws<UniqueConstraintViolationException>(
                () => transaction.Insert(table, Encode(id, "duplicate"))
            );
            Assert.Throws<InvalidOperationException>(() => datastore.Begin());
            transaction.Commit();
            Assert.Throws<InvalidOperationException>(() => transaction.Count(table));
        }

        using (var transaction = datastore.Begin())
        {
            var table = transaction.TableId("entries");
            using var scan = Reader(transaction.Scan(table));
            var rows = new SpacetimeDB.BSATN.Array<byte[], ByteArray>().Read(scan);
            using var row = Reader(Assert.Single(rows));
            var entry = new Entry.BSATN().Read(row);
            Assert.Equal(id, entry.Id);
            Assert.Equal("hello", entry.Text);
            Assert.Equal(scan.BaseStream.Length, scan.BaseStream.Position);
            Assert.Equal(row.BaseStream.Length, row.BaseStream.Position);
            transaction.Insert(table, Encode(0, "rolled back"));
            transaction.Rollback();
        }

        using (var transaction = datastore.Begin())
        {
            var table = transaction.TableId("entries");
            Assert.Equal(1UL, transaction.Count(table));
            transaction.Insert(table, Encode(0, "disposed"));
        }

        using var pending = datastore.Begin();
        Assert.Equal(1UL, pending.Count(pending.TableId("entries")));
        datastore.Dispose();
        Assert.Throws<ObjectDisposedException>(() => pending.TableId("entries"));
        Assert.Throws<ObjectDisposedException>(() => datastore.Begin());
    }

    [Fact]
    public void FinalizersRespectHandleLifetimes()
    {
        using var datastore = NativeDatastore.Create(Schema.Build());
        AbandonTransaction(datastore);
        GC.Collect();
        GC.WaitForPendingFinalizers();

        using var transaction = datastore.Begin();
        Assert.Equal(0UL, transaction.Count(transaction.TableId("entries")));

        using var retained = BeginWithUnrootedDatastore(out var owner);
        GC.Collect();
        GC.WaitForPendingFinalizers();
        Assert.True(owner.TryGetTarget(out var retainedDatastore));
        using (retainedDatastore)
            Assert.Equal(0UL, retained.Count(retained.TableId("entries")));
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static NativeTransaction BeginWithUnrootedDatastore(
        out WeakReference<NativeDatastore> owner
    )
    {
        var datastore = NativeDatastore.Create(Schema.Build());
        owner = new(datastore);
        return datastore.Begin();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void AbandonTransaction(NativeDatastore datastore)
    {
        var transaction = datastore.Begin();
        transaction.Insert(transaction.TableId("entries"), Encode(0, "abandoned"));
    }

    [Fact]
    public void InvalidSchemaAndAbiMismatchHaveDiagnostics()
    {
        var schemaError = Assert.Throws<ArgumentException>(() => NativeDatastore.Create([]));
        Assert.False(string.IsNullOrWhiteSpace(schemaError.Message));
        var abiError = Assert.Throws<InvalidOperationException>(() => Native.ValidateVersion(0));
        Assert.Contains("expected 1, found 0", abiError.Message);
        Assert.Contains("Restore matching", abiError.Message);
    }

    [Fact]
    public void MissingLibraryHasActionableDiagnostic()
    {
        // Isolate P/Invoke resolution from the real library used by the other tests.
        var context = new AssemblyLoadContext("missing-native-library", isCollectible: true);
        try
        {
            var assembly = context.LoadFromAssemblyPath(typeof(NativeDatastore).Assembly.Location);
            NativeLibrary.SetDllImportResolver(
                assembly,
                (_, _, _) => throw new DllNotFoundException("test: missing native asset")
            );
            var method = assembly
                .GetType(typeof(Native).FullName!)!
                .GetMethod("EnsureAvailable", BindingFlags.Static | BindingFlags.NonPublic)!;
            var invocation = Assert.Throws<TargetInvocationException>(
                () => method.Invoke(null, null)
            );
            var error = Assert.IsType<DllNotFoundException>(invocation.InnerException);
            Assert.Contains(RuntimeInformation.RuntimeIdentifier, error.Message);
            Assert.Contains("test output", error.Message);
            Assert.IsType<DllNotFoundException>(error.InnerException);
        }
        finally
        {
            context.Unload();
        }
    }

    private static byte[] Encode(ulong id, string text) =>
        IStructuralReadWrite.ToBytes(new Entry.BSATN(), new Entry { Id = id, Text = text });

    private static BinaryReader Reader(byte[] bytes) => new(new MemoryStream(bytes));
}
