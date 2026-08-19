using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse ListItems(HandlerContext ctx, HttpRequest request) => new(
        200, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString("list")
    );

    [SpacetimeDB.HttpHandler]
    public static HttpResponse CreateItem(HandlerContext ctx, HttpRequest request) => new(
        201, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString($"created:{request.Body.ToStringUtf8Lossy()}")
    );

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New()
        .Get("/items", Handlers.ListItems)
        .Post("/items", Handlers.CreateItem);
}
