using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey]
        public Identity Identity;
        public string Role;
    }

    [Reducer]
    public static void Register(ReducerContext ctx)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("already registered");
        }
        ctx.Db.User.Insert(new User { Identity = ctx.Sender, Role = "member" });
    }

    [Reducer]
    public static void Promote(ReducerContext ctx, Identity target)
    {
        var caller = ctx.Db.User.Identity.Find(ctx.Sender) ?? throw new Exception("not registered");
        if (caller.Role != "admin")
        {
            throw new Exception("not admin");
        }
        var targetUser = ctx.Db.User.Identity.Find(target) ?? throw new Exception("target not registered");
        ctx.Db.User.Identity.Update(targetUser with { Role = "admin" });
    }

    [Reducer]
    public static void MemberAction(ReducerContext ctx)
    {
        _ = ctx.Db.User.Identity.Find(ctx.Sender) ?? throw new Exception("not registered");
    }

    [Reducer]
    public static void AdminAction(ReducerContext ctx)
    {
        var user = ctx.Db.User.Identity.Find(ctx.Sender) ?? throw new Exception("not registered");
        if (user.Role != "admin")
        {
            throw new Exception("not admin");
        }
    }
}
