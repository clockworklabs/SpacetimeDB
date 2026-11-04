using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Quota", Public = true)]
    public partial struct Quota
    {
        [PrimaryKey]
        public Identity Owner;
        public ulong Used;
    }

    [Table(Accessor = "AcceptedRequest", Public = true)]
    public partial struct AcceptedRequest
    {
        [PrimaryKey]
        public string RequestId;
        public Identity Owner;
        public ulong Units;
    }

    [Reducer]
    public static void Submit(ReducerContext ctx, string requestId, long units)
    {
        var owner = ctx.Sender;
        if (ctx.Db.AcceptedRequest.RequestId.Find(requestId) is AcceptedRequest old)
        {
            if (old.Owner == owner && units > 0 && old.Units == (ulong)units)
                return;
            throw new Exception("request conflict");
        }
        if (requestId.Length == 0 || units <= 0)
            throw new Exception("invalid request");
        var used = ctx.Db.Quota.Owner.Find(owner)?.Used ?? 0;
        if ((ulong)units > 3 - used)
            throw new Exception("quota exceeded");
        var next = new Quota { Owner = owner, Used = used + (ulong)units };
        if (ctx.Db.Quota.Owner.Find(owner) is null)
            ctx.Db.Quota.Insert(next);
        else
            ctx.Db.Quota.Owner.Update(next);
        ctx.Db.AcceptedRequest.Insert(
            new AcceptedRequest
            {
                RequestId = requestId,
                Owner = owner,
                Units = (ulong)units,
            }
        );
    }
}
