namespace Runtime.Tests;

using SpacetimeDB;

public class RouterTests
{
    private static class TestHandlers
    {
        public static readonly Handler GetHandler = new(nameof(RouterTests.GetHandler));
        public static readonly Handler PostHandler = new(nameof(RouterTests.PostHandler));
    }

    [Fact]
    public void AllowsDistinctMethodsOnSamePath()
    {
        var router = Router
            .New()
            .Get("/hooks", TestHandlers.GetHandler)
            .Post("/hooks", TestHandlers.PostHandler);

        Assert.NotNull(router);
    }

    [Fact]
    public void RejectsAnyConflictOnSamePath()
    {
        var ex = Assert.Throws<ArgumentException>(
            () =>
                Router
                    .New()
                    .Any("/hooks", TestHandlers.GetHandler)
                    .Get("/hooks", TestHandlers.PostHandler)
        );

        Assert.Contains("Route conflict", ex.Message);
    }

    [Fact]
    public void RejectsInvalidPathCharacters()
    {
        var ex = Assert.Throws<ArgumentException>(
            () => Router.New().Get("/Bad", TestHandlers.GetHandler)
        );

        Assert.Contains("Route paths may contain only", ex.Message);
    }

    [Fact]
    public void NestJoinsPathsWithoutDoubleSlash()
    {
        var router = Router.New().Nest("/api", Router.New().Get("/hooks", TestHandlers.GetHandler));

        Assert.NotNull(router);
    }

    [Fact]
    public void NestRejectsExistingSiblingPrefix()
    {
        var ex = Assert.Throws<ArgumentException>(
            () =>
                Router
                    .New()
                    .Get("/apiv2", TestHandlers.GetHandler)
                    .Nest("/api", Router.New().Get("/hooks", TestHandlers.PostHandler))
        );

        Assert.Contains("Cannot nest router", ex.Message);
    }

    [Fact]
    public void NestRejectsExistingRouteAtNestedPrefix()
    {
        var ex = Assert.Throws<ArgumentException>(
            () =>
                Router
                    .New()
                    .Get("/api", TestHandlers.GetHandler)
                    .Nest("/api", Router.New().Get("/hooks", TestHandlers.PostHandler))
        );

        Assert.Contains("Cannot nest router", ex.Message);
    }

    [Fact]
    public void NestStillRejectsExactRouteConflicts()
    {
        var ex = Assert.Throws<ArgumentException>(
            () =>
                Router
                    .New()
                    .Get("/api/hooks", TestHandlers.GetHandler)
                    .Nest("/api", Router.New().Get("/hooks", TestHandlers.PostHandler))
        );

        Assert.Contains("Cannot nest router", ex.Message);
    }

    private sealed class TestHandlerContext() : HandlerContextBase(new System.Random(), default)
    {
        protected override HandlerTxContextBase CreateTxContext(
            SpacetimeDB.Internal.TxContext inner
        ) => throw new NotSupportedException();

        protected internal override LocalBase CreateLocal() => throw new NotSupportedException();
    }

    private static HttpResponse GetHandler(TestHandlerContext _, HttpRequest __) => default;

    private static HttpResponse PostHandler(TestHandlerContext _, HttpRequest __) => default;
}
