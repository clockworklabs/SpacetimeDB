using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Admin")]
    public partial struct Admin
    {
        [PrimaryKey]
        public Identity Identity;
    }

    [Table(Accessor = "Banned")]
    public partial struct Banned
    {
        [PrimaryKey]
        public Identity Identity;
    }

    [Table(Accessor = "Player", Public = true)]
    public partial struct Player
    {
        [PrimaryKey]
        public Identity Identity;
        public string Name;
    }

    [Reducer]
    public static void AddAdmin(ReducerContext ctx, Identity target)
    {
        if (ctx.Db.Admin.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("not admin");
        }
        try { ctx.Db.Admin.Insert(new Admin { Identity = target }); } catch { }
    }

    [Reducer]
    public static void BanPlayer(ReducerContext ctx, Identity target)
    {
        if (ctx.Db.Admin.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("not admin");
        }
        ctx.Db.Banned.Insert(new Banned { Identity = target });
        if (ctx.Db.Player.Identity.Find(target) is not null)
        {
            ctx.Db.Player.Identity.Delete(target);
        }
    }

    [Reducer]
    public static void JoinGame(ReducerContext ctx, string name)
    {
        if (ctx.Db.Banned.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("banned");
        }
        if (ctx.Db.Player.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("already in game");
        }
        ctx.Db.Player.Insert(new Player { Identity = ctx.Sender, Name = name });
    }
}
