using SpacetimeDB;

[assembly: Namespace(typeof(AuthLib.Functions), Accessor = "MyAuth")]

namespace NamespaceRoot;

[SpacetimeDB.Env]
public struct EnvironmentSchema
{
    public string? NAMESPACE_TEST;
}

public static partial class Functions
{
    [Procedure]
    public static string ReadEnvironment(ProcedureContext ctx)
    {
        var value = ctx.Env.NAMESPACE_TEST;
        if (AuthLib.Functions.ReadEnvironment(ctx) != (value ?? "unset"))
        {
            throw new Exception("Library helper must retain root environment access");
        }

        if (AuthLib.Functions.ReadEnvironmentInTx(ctx) != (value ?? "unset"))
        {
            throw new Exception("Library transaction helper must retain root environment access");
        }

        return ctx.WithTx(tx =>
        {
            if (tx.Env.NAMESPACE_TEST != value)
            {
                throw new Exception("Root transaction environment mismatch");
            }

            return value ?? "unset";
        });
    }

    [HttpRouter]
    public static Router Routes() =>
        Router.New().Get("/root-environment", new Handler(nameof(RootEnvironment)));

    [Reducer]
    public static void ExpectEnvironment(ReducerContext ctx, string? expected)
    {
        if (ctx.Env.NAMESPACE_TEST != expected)
        {
            throw new Exception("Root environment mismatch");
        }

        AuthLib.Functions.ExpectEnvironment(ctx, expected);
    }

    [Procedure]
    public static string? ReadEnvironmentKey(ProcedureContext ctx, string key) => ctx.Env.Get(key);

    [HttpHandler]
    public static HttpResponse RootEnvironment(HandlerContext ctx, HttpRequest request) =>
        AuthLib.Functions.EnvironmentHandler(ctx, request);

    [View(Accessor = "EnvironmentValue", Public = true)]
    public static AuthLib.EnvironmentValue? EnvironmentValue(ViewContext ctx) =>
        AuthLib.Functions.EnvironmentValue(ctx);

    [View(Accessor = "AnonymousEnvironmentValue", Public = true)]
    public static AuthLib.EnvironmentValue? AnonymousEnvironmentValue(AnonymousViewContext ctx) =>
        AuthLib.Functions.AnonymousEnvironmentValue(ctx);
}
