
using System.Collections.Generic;
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Data", Name = "data", Public = true)]
    public partial struct Data
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        public byte[] Body;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Insert(HandlerContext ctx, HttpRequest request)
    {
        var body = request.Body.ToBytes();
        var id = ctx.WithTx((HandlerTxContext tx) => tx.Db.Data.Insert(new Data { Id = 0, Body = body }).Id);
        return TextResponse(200, id.ToString());
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Retrieve(HandlerContext ctx, HttpRequest request)
    {
        var idText = request.Uri.Split("id=", 2)[1];
        var id = ulong.Parse(idText);
        var body = ctx.WithTx((HandlerTxContext tx) => tx.Db.Data.Id.Find(id)?.Body);

        if (body is not null)
        {
            return BytesResponse(200, body);
        }

        return new HttpResponse(404, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.Empty);
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Post("/insert", Handlers.Insert)
            .Get("/retrieve", Handlers.Retrieve);

    private static HttpResponse BytesResponse(ushort statusCode, byte[] body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), new HttpBody(body));

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
