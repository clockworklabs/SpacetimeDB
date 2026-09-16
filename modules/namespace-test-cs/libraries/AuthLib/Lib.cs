using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

namespace AuthLib;

public class Marker { }

[Table(Accessor = "User", Name = "auth_users", Public = true)]
[SpacetimeDB.Index.BTree(Accessor = "ByScore", Columns = [nameof(Score)])]
public partial struct User
{
    [PrimaryKey]
    public uint Id;
    public uint Score;
}

public static partial class Functions
{
    public static void Insert(ReducerContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Score = 42 });

    public static ulong Count(ReducerContext ctx) => ctx.Db.User.Count;

    [Reducer]
    public static void Add(ReducerContext ctx, uint id) => Insert(ctx, id);

    [Procedure]
    public static ulong CountUsers(ProcedureContext ctx) => ctx.WithTx(tx => tx.Db.User.Count);

    [View(Accessor = "Users", Public = true)]
    public static User? Users(ViewContext ctx) => ctx.Db.User.Id.Find(2);

    [View(Accessor = "AnonymousUsers", Public = true)]
    public static User? AnonymousUsers(AnonymousViewContext ctx) => ctx.Db.User.Id.Find(2);

    [HttpHandler]
    public static HttpResponse AuthCount(HandlerContext ctx, HttpRequest request) =>
        new(
            200,
            HttpVersion.Http11,
            [],
            HttpBody.FromString(ctx.WithTx(tx => tx.Db.User.Count).ToString())
        );

    [HttpRouter]
    public static Router Routes() =>
        Router.New().Get("/auth-count", new Handler(nameof(AuthCount)));
}
