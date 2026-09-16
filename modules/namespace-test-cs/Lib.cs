using SpacetimeDB;

[assembly: Namespace(typeof(AuthLib.Marker), Accessor = "MyAuth", Name = "auth_data")]
[assembly: Namespace(typeof(AuditLib.Marker), Accessor = "class", Name = "audit_data")]

namespace NamespaceRoot;

[Table(Accessor = "User", Public = true)]
public partial struct User
{
    [PrimaryKey]
    public uint Id;
}

[SpacetimeDB.Type]
public partial struct AuthSummary
{
    public uint Score;
}

public static partial class Functions
{
    [Reducer]
    public static void Exercise(ReducerContext ctx)
    {
        ctx.Db.User.Insert(new User { Id = 1 });
        AuthLib.Functions.Insert(ctx, 2);
        ctx.Db.MyAuth.User.Insert(new AuthLib.User { Id = 3, Score = 43 });
        ctx.Db.@class.User.Insert(new AuditLib.User { Id = 4, Message = "consumer" });
        if (
            ctx.Db.User.Count != 1
            || AuthLib.Functions.Count(ctx) != 2
            || ctx.Db.@class.User.Count != 1
            || ctx.Db.ExtraRow.Count != 1
        )
            throw new Exception("Namespace counts are not isolated.");
        var row = ctx.Db.MyAuth.User.Id.Find(2)!.Value;
        row.Score = 99;
        ctx.Db.MyAuth.User.Id.Update(row);
        if (ctx.Db.MyAuth.User.ByScore.Filter(99u).Single().Id != 2)
            throw new Exception("Mounted index lookup failed.");
        if (!ctx.Db.MyAuth.User.Id.Delete(3) || ctx.Db.MyAuth.User.Count != 1)
            throw new Exception("Mounted unique deletion failed.");
        Log.Info("namespace composition works");
    }

    [Procedure]
    public static ulong CountUsers(ProcedureContext ctx) =>
        ctx.WithTx(tx => tx.Db.User.Count + tx.Db.MyAuth.User.Count + tx.Db.@class.User.Count);

    [View(Accessor = "Users", Public = true)]
    public static User? Users(ViewContext ctx) => ctx.Db.User.Id.Find(1);

    [View(Accessor = "AuthUsers", Public = true)]
    public static List<AuthSummary> AuthUsers(ViewContext ctx) =>
        ctx
            .Db.MyAuth.User.ByScore.Filter(99u)
            .Select(row => new AuthSummary { Score = row.Score })
            .ToList();
}
