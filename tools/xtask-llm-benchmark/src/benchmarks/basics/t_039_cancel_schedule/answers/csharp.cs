using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "CleanupJob", Scheduled = nameof(RunCleanup), ScheduledAt = nameof(CleanupJob.ScheduledAt))]
    public partial struct CleanupJob
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    [Reducer]
    public static void RunCleanup(ReducerContext ctx, CleanupJob row) { }

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.CleanupJob.Insert(new CleanupJob
        {
            ScheduledAt = new ScheduleAt.Interval(new TimeDuration { Microseconds = 60_000_000 }),
        });
    }

    [Reducer]
    public static void CancelCleanup(ReducerContext ctx, ulong scheduledId)
    {
        ctx.Db.CleanupJob.ScheduledId.Delete(scheduledId);
    }
}
