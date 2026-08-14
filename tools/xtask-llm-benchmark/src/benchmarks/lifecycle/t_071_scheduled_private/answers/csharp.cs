using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "JobResult", Public = true)]
    public partial struct JobResult { [PrimaryKey] public ulong Id; public string Status; }

    [Table(Accessor = "PrivateJob", Scheduled = nameof(RunPrivateJob), ScheduledAt = nameof(PrivateJob.ScheduledAt))]
    public partial struct PrivateJob
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong ResultId;
    }

    [Reducer]
    public static void EnqueuePrivate(ReducerContext ctx, ulong id)
    {
        ctx.Db.JobResult.Insert(new JobResult { Id = id, Status = "queued" });
        ctx.Db.PrivateJob.Insert(new PrivateJob {
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration { Microseconds = 1_000 }), ResultId = id,
        });
    }

    [Reducer]
    public static void RunPrivateJob(ReducerContext ctx, PrivateJob job)
    {
        var result = ctx.Db.JobResult.Id.Find(job.ResultId) ?? throw new InvalidOperationException("job result missing");
        result.Status = "complete";
        ctx.Db.JobResult.Id.Update(result);
    }
}
