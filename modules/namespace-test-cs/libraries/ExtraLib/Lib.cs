using SpacetimeDB;

namespace ExtraLib;

[Table(Public = true)]
public partial struct ExtraRow
{
    [PrimaryKey]
    public uint Id;
}

public static partial class Functions
{
    [Reducer]
    public static void Extra(ReducerContext ctx) => ctx.Db.ExtraRow.Insert(new ExtraRow { Id = 7 });

    [View(Accessor = "ExtraRows", Public = true)]
    public static ExtraRow? Rows(ViewContext ctx) => ctx.Db.ExtraRow.Id.Find(7);

    [HttpHandler]
    public static HttpResponse ExtraHello(HandlerContext ctx, HttpRequest request) =>
        new(200, HttpVersion.Http11, [], HttpBody.FromString("public"));

    [HttpRouter]
    public static Router Routes() => Router.New().Get("/extra-hello", Handlers.ExtraHello);
}
