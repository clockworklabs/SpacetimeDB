
using System;
using System.Collections.Generic;
using System.Text;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse ReverseBytes(HandlerContext ctx, HttpRequest request)
    {
        var reversed = request.Body.ToBytes();
        Array.Reverse(reversed);
        return BytesResponse(200, reversed);
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse ReverseWords(HandlerContext ctx, HttpRequest request)
    {
        string body;
        try
        {
            body = new UTF8Encoding(false, true).GetString(request.Body.ToBytes());
        }
        catch (DecoderFallbackException)
        {
            return TextResponse(400, "request body must be valid UTF-8");
        }

        var reversed = string.Join(" ", body.Split(' ').Reverse());
        return TextResponse(200, reversed);
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Post("/reverse-bytes", Handlers.ReverseBytes)
            .Post("/reverse-words", Handlers.ReverseWords);

    private static HttpResponse BytesResponse(ushort statusCode, byte[] body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), new HttpBody(body));

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
