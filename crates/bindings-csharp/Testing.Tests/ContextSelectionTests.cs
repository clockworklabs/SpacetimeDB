namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class ContextSelectionTests
{
    // Converting a selector back to its context type is what module code relies on.
    private static void CheckSelected<TContext>(TContext left, TContext right)
        where TContext : IModuleContext
    {
        var namespaces = Schema.CreateNamespaces();
        Assert.Equal("Left.Leaf.probe", namespaces.Resolve(left.InstanceId, "probe"));
        Assert.Equal("Right.Leaf.probe", namespaces.Resolve(right.InstanceId, "probe"));
    }

    [Fact]
    public void GeneratedSelectorsUseTheFixtureWithoutInstallingGlobalNamespaces()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx => CheckSelected<ReducerContext>(ctx.As.Left.Leaf, ctx.As.Right.Leaf)
        );
        var view = test.CreateViewContext(TestAuth.Internal());
        CheckSelected<ViewContext>(view.As.Left.Leaf, view.As.Right.Leaf);
        var anonymous = test.CreateAnonymousViewContext();
        CheckSelected<AnonymousViewContext>(anonymous.As.Left.Leaf, anonymous.As.Right.Leaf);
        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        CheckSelected<ProcedureContext>(procedure.As.Left.Leaf, procedure.As.Right.Leaf);
        procedure.WithTx(tx =>
        {
            CheckSelected<ProcedureTxContext>(tx.As.Left.Leaf, tx.As.Right.Leaf);
            return 0;
        });
        Assert.Throws<InvalidOperationException>(() => Module.InstanceCount);
    }

    [Fact]
    public void GeneratedSelectorsRejectExpiredOwners()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var reducerPath = test.WithReducerTx(TestAuth.Internal(), ctx => ctx.As.Left);
        Assert.Throws<ObjectDisposedException>(() => reducerPath.Leaf);

        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        var transactionPath = procedure.WithTx(tx => tx.As.Left);
        Assert.Throws<ObjectDisposedException>(() => transactionPath.Leaf);

        var viewPath = test.CreateViewContext(TestAuth.Internal()).As.Left;
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => viewPath.Leaf);
    }
}
