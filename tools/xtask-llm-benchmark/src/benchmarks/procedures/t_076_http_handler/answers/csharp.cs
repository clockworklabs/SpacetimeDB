using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse Echo(HandlerContext ctx, HttpRequest request) => new(
        201, HttpVersion.Http11,
        new List<HttpHeader> { new("content-type", System.Text.Encoding.UTF8.GetBytes("text/plain")) },
        HttpBody.FromString($"echo:{request.Body.ToStringUtf8Lossy()}")
    );

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New().Post("/echo", Handlers.Echo);
}
