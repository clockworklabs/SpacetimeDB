namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class OwnershipTests
{
    [Fact]
    public void FixtureAndTransactionAccessHaveIndependentLifetimes()
    {
        var namespaces = Schema.CreateNamespaces();
        using var backend = new DatabaseBackend(Schema.Build(), namespaces);
        using var otherBackend = new DatabaseBackend(Schema.Build(), Schema.CreateNamespaces());
        var fixture = new DatabaseAccess(backend);
        var otherFixture = new DatabaseAccess(otherBackend);
        Assert.Same(namespaces, fixture.Backend.Namespaces);
        Assert.NotSame(namespaces, otherFixture.Backend.Namespaces);

        using var transaction = backend.BeginTransaction();
        var access = new DatabaseAccess(transaction);
        var db = new SpacetimeDB.Local(access);
        var auth = AuthCtx.BuildFromSystemTables(null, default);
        var context = new ReducerContext(db, default, null, new Random(0), default, auth);
        Assert.Same(db, context.Db);
        Assert.Same(access, context.Db.Access);
        Assert.Same(auth, context.SenderAuth);
        Assert.Equal(default, context.DatabaseIdentity);

        transaction.Commit();
        using var next = backend.BeginTransaction();
        Assert.Throws<InvalidOperationException>(() => access.CheckActive());
        Assert.Throws<InvalidOperationException>(() => context.DatabaseIdentity);
        fixture.CheckActive();
        var nextAccess = new DatabaseAccess(next);
        next.Rollback();
        Assert.Throws<InvalidOperationException>(() => nextAccess.CheckActive());

        using var pending = backend.BeginTransaction();
        var pendingAccess = new DatabaseAccess(pending);
        pending.Dispose();
        Assert.Throws<ObjectDisposedException>(() => pendingAccess.CheckActive());
        fixture.CheckActive();

        backend.Dispose();
        Assert.Throws<ObjectDisposedException>(() => fixture.CheckActive());
        Assert.Throws<ObjectDisposedException>(() => backend.BeginTransaction());
        otherFixture.CheckActive();
        using var independent = otherBackend.BeginTransaction();
        independent.Commit();
    }

    [Fact]
    public void DefaultContextsAreUnbound()
    {
        Assert.Null(new SpacetimeDB.Local().Access);
        Assert.Null(new LocalReadOnly().Access);
        var context = new ReducerContext(default, null, new Random(0), default);
        Assert.Null(context.Db.Access);
        Assert.True(context.SenderAuth.IsInternal);
        Assert.Throws<InvalidOperationException>(() => context.DatabaseIdentity);
    }
}
