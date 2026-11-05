namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class ContextSelectionTests
{
    private static void CheckSelected(IModuleContext left, IModuleContext right)
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
            ctx =>
            {
                ReducerContext left = ctx.As.Left.Leaf;
                ReducerContext right = ctx.As.Right.Leaf;
                CheckSelected(left, right);
                Assert.Same(ctx.Db.Access, left.Db.Access);
                Assert.Same(ctx.Db.Access, right.Db.Access);
            }
        );

        var view = test.CreateViewContext(TestAuth.Internal());
        ViewContext leftView = view.As.Left.Leaf;
        ViewContext rightView = view.As.Right.Leaf;
        CheckSelected(leftView, rightView);
        Assert.Same(view.Db.Access, leftView.Db.Access);

        var anonymous = test.CreateAnonymousViewContext();
        AnonymousViewContext leftAnonymous = anonymous.As.Left.Leaf;
        AnonymousViewContext rightAnonymous = anonymous.As.Right.Leaf;
        CheckSelected(leftAnonymous, rightAnonymous);
        Assert.Same(anonymous.Db.Access, leftAnonymous.Db.Access);

        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        ProcedureContext leftProcedure = procedure.As.Left.Leaf;
        ProcedureContext rightProcedure = procedure.As.Right.Leaf;
        CheckSelected(leftProcedure, rightProcedure);
        Assert.Null(leftProcedure.Db.Access);
        procedure.WithTx(tx =>
        {
            ProcedureTxContext left = tx.As.Left.Leaf;
            ProcedureTxContext right = tx.As.Right.Leaf;
            CheckSelected(left, right);
            Assert.Same(tx.Db.Access, left.Db.Access);
            return 0;
        });
        leftProcedure.WithTx(tx =>
        {
            Assert.Equal(
                ((IModuleContext)leftProcedure).InstanceId,
                ((IModuleContext)tx).InstanceId
            );
            return 0;
        });
        Assert.Throws<InvalidOperationException>(() => Module.InstanceCount);
    }

    [Fact]
    public void GeneratedSelectorsRejectExpiredOwners()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        Action? selectFromReducer = null;
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                var path = ctx.As.Left;
                selectFromReducer = () =>
                {
                    _ = path.Leaf;
                };
            }
        );
        Assert.Throws<ObjectDisposedException>(selectFromReducer!);

        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        Action? selectFromTransaction = null;
        procedure.WithTx(tx =>
        {
            var path = tx.As.Left;
            selectFromTransaction = () =>
            {
                _ = path.Leaf;
            };
            return 0;
        });
        Assert.Throws<ObjectDisposedException>(selectFromTransaction!);

        var procedurePath = procedure.As.Left;
        var viewPath = test.CreateViewContext(TestAuth.Internal()).As.Left;
        var anonymousPath = test.CreateAnonymousViewContext().As.Left;
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => procedurePath.Leaf);
        Assert.Throws<ObjectDisposedException>(() => viewPath.Leaf);
        Assert.Throws<ObjectDisposedException>(() => anonymousPath.Leaf);

        using var other = ModuleTestContext.Create(typeof(Entry).Assembly);
        var otherView = other.CreateViewContext(TestAuth.Internal());
        ViewContext left = otherView.As.Left.Leaf;
        ViewContext right = otherView.As.Right.Leaf;
        CheckSelected(left, right);
    }
}
