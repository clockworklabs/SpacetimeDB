namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class EnvironmentTests
{
    [Fact]
    public void ValuesAreLiveFixtureConfigurationNotTransactionalWrites()
    {
        using var first = ModuleTestContext.Create(typeof(Entry).Assembly);
        using var second = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        var view = first.CreateViewContext(auth);
        var anonymous = first.CreateAnonymousViewContext();
        var selectedView = ((IModuleContext<ViewContext>)view).SelectInstance(1);
        var selectedAnonymous = ((IModuleContext<AnonymousViewContext>)anonymous).SelectInstance(1);
        Assert.Equal(1, ((IModuleContext)selectedView).InstanceId);
        Assert.Equal(1, ((IModuleContext)selectedAnonymous).InstanceId);
        Assert.Null(view.Env.OPTIONAL);
        Assert.Throws<InvalidOperationException>(() => view.Env.REQUIRED);
        first.SetEnv("REQUIRED", "");
        Assert.Equal("", view.Env.REQUIRED);

        DatabaseEnvironment escaped = default;
        Assert.Throws<InvalidOperationException>(
            () =>
                first.WithReducerTx(
                    auth,
                    ctx =>
                    {
                        escaped = ctx.Env;
                        first.SetEnv("OPTIONAL", "changed");
                        Assert.Equal("changed", escaped.OPTIONAL);
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal("changed", view.Env.OPTIONAL);
        Assert.Equal("changed", anonymous.Env.OPTIONAL);
        Assert.Equal("changed", selectedView.Env.OPTIONAL);
        Assert.Equal("changed", selectedAnonymous.Env.OPTIONAL);
        second.WithReducerTx(auth, ctx => Assert.Null(ctx.Env.OPTIONAL));
        Assert.Throws<ObjectDisposedException>(() => escaped.Get("OPTIONAL"));

        first.UnsetEnv("OPTIONAL");
        first.UnsetEnv("OPTIONAL");
        Assert.Null(view.Env.OPTIONAL);
        first.UnsetEnv("REQUIRED");
        Assert.Throws<InvalidOperationException>(() => view.Env.REQUIRED);
        first.Dispose();
        Assert.Throws<ObjectDisposedException>(() => view.Env.Get("OPTIONAL"));
        Assert.Throws<ObjectDisposedException>(() => selectedView.Env.Get("OPTIONAL"));
        Assert.Throws<ObjectDisposedException>(() => selectedAnonymous.Env.Get("OPTIONAL"));
        Assert.Throws<ObjectDisposedException>(() => first.SetEnv("OPTIONAL", "value"));
        Assert.Throws<ObjectDisposedException>(() => first.UnsetEnv("OPTIONAL"));
        Assert.Throws<InvalidOperationException>(
            () => default(DatabaseEnvironment).Get("OPTIONAL")
        );
    }

    [Fact]
    public void ValuesMustMatchDeclarationsAndUtf8Limits()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        test.SetEnv("LITERAL", "fixed");
        test.SetEnv("MODE", "development");
        test.SetEnv("MODE", "production");
        Assert.Throws<ArgumentException>(() => test.SetEnv("LITERAL", "other"));
        Assert.Throws<ArgumentException>(() => test.SetEnv("MODE", "Production"));
        Assert.Throws<ArgumentNullException>(() => test.SetEnv("OPTIONAL", null!));
        Assert.Throws<ArgumentException>(() => test.SetEnv("optional", "value"));
        Assert.Throws<ArgumentException>(() => test.UnsetEnv("optional"));
        var limit = new string('\u00e9', 4096);
        test.SetEnv("OPTIONAL", limit);
        Assert.Throws<ArgumentException>(() => test.SetEnv("OPTIONAL", limit + "x"));
        test.WithReducerTx(
            auth,
            ctx =>
            {
                Assert.Throws<ArgumentException>(() => ctx.Env.Get("optional"));
                Assert.Equal("fixed", ctx.Env.LITERAL);
                Assert.Equal("production", ctx.Env.MODE);
                Assert.Equal(limit, ctx.Env.OPTIONAL);
            }
        );
        test.SetEnv("OPTIONAL", "\0");
        test.WithReducerTx(auth, ctx => Assert.Equal("\0", ctx.Env.OPTIONAL));
    }
}
