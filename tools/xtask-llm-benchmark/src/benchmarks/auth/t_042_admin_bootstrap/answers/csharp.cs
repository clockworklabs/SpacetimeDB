using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Config")]
    public partial struct Config
    {
        [PrimaryKey]
        public uint Id;
        public Identity Admin;
    }

    [Table(Accessor = "AdminLog", Public = true)]
    public partial struct AdminLog
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Action;
    }

    [Reducer]
    public static void BootstrapAdmin(ReducerContext ctx)
    {
        if (ctx.Db.Config.Id.Find(0u) is not null)
        {
            throw new Exception("already bootstrapped");
        }
        ctx.Db.Config.Insert(new Config { Id = 0, Admin = ctx.Sender });
    }

    [Reducer]
    public static void AdminAction(ReducerContext ctx, string action)
    {
        var config = ctx.Db.Config.Id.Find(0u) ?? throw new Exception("not bootstrapped");
        if (config.Admin != ctx.Sender)
        {
            throw new Exception("not admin");
        }
        ctx.Db.AdminLog.Insert(new AdminLog { Id = 0, Action = action });
    }
}
