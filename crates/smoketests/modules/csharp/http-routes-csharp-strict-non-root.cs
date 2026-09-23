
using System.Collections.Generic;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse Foo(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse FooSlash(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo-slash");
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Get("/foo", Handlers.Foo)
            .Get("/foo/", Handlers.FooSlash);

    private static HttpResponse TextResponse(string body) =>
        new(200, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
