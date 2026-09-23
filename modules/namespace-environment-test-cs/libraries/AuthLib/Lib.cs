using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

namespace AuthLib;

[SpacetimeDB.Type]
public partial struct EnvironmentValue
{
    public string Value;
}

public static partial class Functions
{
    [Procedure]
    public static string ReadEnvironment(ProcedureContext ctx) =>
        ctx.Env.Get("NAMESPACE_TEST") ?? "unset";

    [HttpRouter]
    public static Router Routes() =>
        Router.New().Get("/auth-environment", new Handler(nameof(EnvironmentHandler)));

    [Reducer]
    public static void ExpectEnvironment(ReducerContext ctx, string? expected)
    {
        if (ctx.Env.Get("NAMESPACE_TEST") != expected)
            throw new Exception("Library environment mismatch");
    }

    [Procedure]
    public static string ReadEnvironmentInTx(ProcedureContext ctx) =>
        ctx.WithTx(tx => tx.Env.Get("NAMESPACE_TEST") ?? "unset");

    [HttpHandler]
    public static HttpResponse EnvironmentHandler(HandlerContext ctx, HttpRequest request)
    {
        var value = ctx.Env.Get("NAMESPACE_TEST");
        if (ctx.WithTx(tx => tx.Env.Get("NAMESPACE_TEST")) != value)
            throw new Exception("Handler transaction environment mismatch");
        return new(200, HttpVersion.Http11, [], HttpBody.FromString(value ?? "unset"));
    }

    [View(Accessor = "EnvironmentValue", Public = true)]
    public static EnvironmentValue? EnvironmentValue(ViewContext ctx) =>
        new EnvironmentValue { Value = ctx.Env.Get("NAMESPACE_TEST") ?? "unset" };

    [View(Accessor = "AnonymousEnvironmentValue", Public = true)]
    public static EnvironmentValue? AnonymousEnvironmentValue(AnonymousViewContext ctx) =>
        new EnvironmentValue { Value = ctx.Env.Get("NAMESPACE_TEST") ?? "unset" };
}
