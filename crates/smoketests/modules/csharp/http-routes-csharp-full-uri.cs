
using System.Collections.Generic;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse EchoUri(HandlerContext ctx, HttpRequest request)
    {
        return new HttpResponse(
            200,
            HttpVersion.Http11,
            new List<HttpHeader>(),
            HttpBody.FromString(request.Uri)
        );
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New().Get("/echo-uri", Handlers.EchoUri);
}
