using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(TickTimer.ScheduledAt))]
    public partial struct TickTimer
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt     ScheduledAt;
    }

    [Reducer]
    public static void Tick(ReducerContext ctx, TickTimer timer) { }

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var interval = new TimeDuration { Microseconds = 50_000 };
        ctx.Db.TickTimer.Insert(new TickTimer
        {
            ScheduledAt = new ScheduleAt.Interval(interval)
        });
    }
}
