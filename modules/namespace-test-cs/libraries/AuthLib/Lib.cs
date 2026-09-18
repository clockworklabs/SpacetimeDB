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

[Table(Accessor = "Notice", Public = true, Event = true)]
public partial struct Notice
{
    public uint Id;
}

[Table(Accessor = "Secret")]
public partial struct Secret
{
    public uint Id;
}

public static partial class Functions
{
    public static void Insert(ReducerContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Score = 42 });

    public static void Insert(ProcedureTxContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Score = 42 });

    public static ulong Count(ReducerContext ctx) => ctx.Db.User.Count;

    public static FromWhere<User, UserCols, UserIxCols> Query(ViewContext ctx) =>
        ctx.From.User().Where(row => row.Score.Eq(99u));

    [View(Accessor = "QueryUsers", Public = true)]
    public static IQuery<User> QueryUsers(ViewContext ctx) => Query(ctx);

    [Reducer]
    public static void Add(ReducerContext ctx, uint id)
    {
        Insert(ctx, id);
        ctx.Db.Notice.Insert(new Notice { Id = id });
        ctx.Db.Secret.Insert(new Secret { Id = id });
    }

    [Reducer]
    public static void Fail(ReducerContext ctx, uint id)
    {
        Insert(ctx, id);
        throw new Exception("namespace rollback");
    }

    [Reducer]
    public static void Update(ReducerContext ctx, uint id, uint score)
    {
        var row = ctx.Db.User.Id.Find(id)!.Value;
        row.Score = score;
        ctx.Db.User.Id.Update(row);
    }

    [Reducer]
    public static void Remove(ReducerContext ctx, uint id) => ctx.Db.User.Id.Delete(id);

    [Procedure]
    public static uint ReadScore(ProcedureContext ctx, uint id) =>
        ctx.WithTx(tx => tx.Db.User.Id.Find(id)?.Score ?? throw new Exception("missing auth user"));

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
