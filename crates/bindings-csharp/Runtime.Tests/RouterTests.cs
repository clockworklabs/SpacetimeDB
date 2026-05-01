namespace Runtime.Tests;

using SpacetimeDB;

public class RouterTests
{
    [Fact]
    public void AllowsDistinctMethodsOnSamePath()
    {
        var router = Router.New()
            .Get("/hooks", nameof(GetHandler))
            .Post("/hooks", nameof(PostHandler));

        Assert.NotNull(router);
    }

    [Fact]
    public void RejectsAnyConflictOnSamePath()
    {
        var ex = Assert.Throws<ArgumentException>(
            () => Router.New().Any("/hooks", nameof(GetHandler)).Get("/hooks", nameof(PostHandler))
        );

        Assert.Contains("Route conflict", ex.Message);
    }

    [Fact]
    public void RejectsInvalidPathCharacters()
    {
        var ex = Assert.Throws<ArgumentException>(
            () => Router.New().Get("/Bad", nameof(GetHandler))
        );

        Assert.Contains("Route paths may contain only", ex.Message);
    }

    [Fact]
    public void NestJoinsPathsWithoutDoubleSlash()
    {
        var router = Router.New().Nest("/api", Router.New().Get("/hooks", nameof(GetHandler)));

        Assert.NotNull(router);
    }

    [Fact]
    public void NestAllowsExistingSiblingPrefix()
    {
        var router = Router.New()
            .Get("/apiv2", nameof(GetHandler))
            .Nest("/api", Router.New().Get("/hooks", nameof(PostHandler)));

        Assert.NotNull(router);
    }

    [Fact]
    public void NestAllowsExistingRouteAtNestedPrefix()
    {
        var router = Router.New()
            .Get("/api", nameof(GetHandler))
            .Nest("/api", Router.New().Get("/hooks", nameof(PostHandler)));

        Assert.NotNull(router);
    }

    [Fact]
    public void NestStillRejectsExactRouteConflicts()
    {
        var ex = Assert.Throws<ArgumentException>(
            () => Router.New().Get("/api/hooks", nameof(GetHandler))
                .Nest("/api", Router.New().Get("/hooks", nameof(PostHandler)))
        );

        Assert.Contains("Route conflict", ex.Message);
    }

    private static void GetHandler() { }

    private static void PostHandler() { }
}
