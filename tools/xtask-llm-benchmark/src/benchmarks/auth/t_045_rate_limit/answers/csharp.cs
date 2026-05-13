using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "RateLimit")]
    public partial struct RateLimit
    {
        [PrimaryKey]
        public Identity Identity;
        public ulong LastCallUs;
    }

    [Table(Accessor = "ActionLog", Public = true)]
    public partial struct ActionLog
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public Identity Identity;
        public string Payload;
    }

    [Reducer]
    public static void LimitedAction(ReducerContext ctx, string payload)
    {
        ulong now = (ulong)ctx.Timestamp.MicrosecondsSinceUnixEpoch;
        var entry = ctx.Db.RateLimit.Identity.Find(ctx.Sender);
        if (entry is RateLimit r)
        {
            if (now - r.LastCallUs < 1_000_000UL)
            {
                throw new Exception("rate limited");
            }
            ctx.Db.RateLimit.Identity.Update(r with { LastCallUs = now });
        }
        else
        {
            ctx.Db.RateLimit.Insert(new RateLimit { Identity = ctx.Sender, LastCallUs = now });
        }
        ctx.Db.ActionLog.Insert(new ActionLog { Id = 0, Identity = ctx.Sender, Payload = payload });
    }
}
