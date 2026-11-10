using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "LeasedJob", Public = true)]
    public partial struct LeasedJob
    {
        [PrimaryKey]
        public ulong Id;
        public string Worker;
        public ulong Generation;
        public long ExpiresAt;
        public bool Done;
        public string Result;
    }

    [Reducer]
    public static void CreateJob(ReducerContext ctx, ulong id)
    {
        ctx.Db.LeasedJob.Insert(
            new LeasedJob
            {
                Id = id,
                Worker = "",
                Result = "",
            }
        );
    }

    [Reducer]
    public static void Claim(ReducerContext ctx, ulong id, string worker, ulong leaseMs)
    {
        if (worker.Length == 0 || leaseMs == 0 || leaseMs > 60000)
            throw new Exception("invalid lease");
        var now = ctx.Timestamp.MicrosecondsSinceUnixEpoch;
        var row = ctx.Db.LeasedJob.Id.Find(id) ?? throw new Exception("missing job");
        if (row.Done)
            throw new Exception("already done");
        if (row.Worker.Length != 0 && now < row.ExpiresAt)
            throw new Exception("lease busy");
        row.Generation++;
        row.Worker = worker;
        row.ExpiresAt = now + (long)leaseMs * 1000;
        ctx.Db.LeasedJob.Id.Update(row);
    }

    [Reducer]
    public static void Complete(
        ReducerContext ctx,
        ulong id,
        string worker,
        ulong generation,
        string result
    )
    {
        var row = ctx.Db.LeasedJob.Id.Find(id) ?? throw new Exception("missing job");
        if (row.Done)
        {
            if (row.Worker == worker && row.Generation == generation && row.Result == result)
                return;
            throw new Exception("already done");
        }
        if (
            worker.Length == 0
            || generation == 0
            || row.Worker != worker
            || row.Generation != generation
            || ctx.Timestamp.MicrosecondsSinceUnixEpoch >= row.ExpiresAt
        )
            throw new Exception("stale lease");
        row.Done = true;
        row.Result = result;
        ctx.Db.LeasedJob.Id.Update(row);
    }
}
