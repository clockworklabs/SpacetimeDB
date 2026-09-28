
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Entry", Name = "entry", Public = true)]
    public partial struct Entry
    {
        [SpacetimeDB.PrimaryKey]
        public ulong Id;

        public string Value;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse GetSimple(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "ok");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse PostInsert(HandlerContext ctx, HttpRequest request)
    {
        ctx.WithTx((HandlerTxContext tx) =>
        {
            var id = tx.Db.Entry.Count;
            tx.Db.Entry.Insert(new Entry { Id = id, Value = "posted" });
            return 0;
        });
        return TextResponse(200, "inserted");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse GetCount(HandlerContext ctx, HttpRequest request)
    {
        var count = ctx.WithTx((HandlerTxContext tx) => tx.Db.Entry.Count);
        return TextResponse(200, count.ToString());
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse AnyHandler(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "any");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse HeaderEcho(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, HeaderValueUtf8(request, "x-echo"));
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse SetResponseHeader(HandlerContext ctx, HttpRequest request)
    {
        return new HttpResponse(
            200,
            HttpVersion.Http11,
            new List<HttpHeader> { new("x-response", "set") },
            HttpBody.FromString("header-set")
        );
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse BodyHandler(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "non-empty");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Teapot(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(418, "teapot");
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Get("/get", Handlers.GetSimple)
            .Post("/post", Handlers.PostInsert)
            .Get("/count", Handlers.GetCount)
            .Any("/any", Handlers.AnyHandler)
            .Get("/header", Handlers.HeaderEcho)
            .Get("/set-header", Handlers.SetResponseHeader)
            .Get("/body", Handlers.BodyHandler)
            .Get("/teapot", Handlers.Teapot);

    private static string HeaderValueUtf8(HttpRequest request, string headerName)
    {
        foreach (var header in request.Headers)
        {
            if (string.Equals(header.Name, headerName, StringComparison.OrdinalIgnoreCase))
            {
                return Encoding.UTF8.GetString(header.Value);
            }
        }
        return string.Empty;
    }

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(
            statusCode,
            HttpVersion.Http11,
            new List<HttpHeader>(),
            HttpBody.FromString(body)
        );
}
