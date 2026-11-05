namespace SpacetimeDB.Testing.Tests;

using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.Loader;
using SpacetimeDB.BSATN;
using TestingModule;

public class InteropTests
{
    [Theory]
    [InlineData(NativeStatus.UniqueViolation, typeof(UniqueConstraintViolationException))]
    [InlineData(NativeStatus.AutoIncOverflow, typeof(AutoIncOverflowException))]
    internal void ConstraintStatusesBecomeTypedExceptions(NativeStatus status, Type expected)
    {
        // The engine only overflows a sequence at i128 limits (narrower columns wrap), so
        // AutoIncOverflow can't be reached through a table. The Rust crate tests that mapping.
        var error = default(NativeBuffer);
        Assert.IsType(expected, Record.Exception(() => Native.Check(status, ref error)));
    }

    [Fact]
    public void FinalizersRespectHandleLifetimes()
    {
        using var datastore = NativeDatastore.Create(TestSchema.Build());
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
        var datastore = NativeDatastore.Create(TestSchema.Build());
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
}
