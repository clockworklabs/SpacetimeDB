using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Reminder", Scheduled = nameof(SendReminder), ScheduledAt = nameof(Reminder.ScheduledAt))]
    public partial struct Reminder
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public string Message;
    }

    [Reducer]
    public static void SendReminder(ReducerContext ctx, Reminder row) { }

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var delay = new TimeDuration { Microseconds = 60_000_000 };
        ctx.Db.Reminder.Insert(new Reminder
        {
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + delay),
            Message = "Hello!",
        });
    }
}
