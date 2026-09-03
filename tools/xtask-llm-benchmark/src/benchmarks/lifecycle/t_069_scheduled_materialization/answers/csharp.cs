using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "MaterializedState", Public = true)]
    public partial struct MaterializedState
    {
        [PrimaryKey] public ulong Id;
        public string Status;
        public ulong Version;
        public Timestamp RefreshedAt;
    }

    [Table(Accessor = "RefreshJob", Scheduled = nameof(RefreshMaterialized), ScheduledAt = nameof(RefreshJob.ScheduledAt))]
    public partial struct RefreshJob
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong StateId;
    }

    [Reducer]
    public static void StartRefresh(ReducerContext ctx)
    {
        var pending = new MaterializedState { Id = 1, Status = "pending", Version = 0, RefreshedAt = new Timestamp(0) };
        if (ctx.Db.MaterializedState.Id.Find(1) is null) ctx.Db.MaterializedState.Insert(pending);
        else ctx.Db.MaterializedState.Id.Update(pending);
        ctx.Db.RefreshJob.Insert(new RefreshJob {
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration { Microseconds = 1_000 }), StateId = 1,
        });
    }

    [Reducer]
    public static void RefreshMaterialized(ReducerContext ctx, RefreshJob job)
    {
        var state = ctx.Db.MaterializedState.Id.Find(job.StateId) ?? throw new InvalidOperationException("materialized state missing");
        state.Status = "ready";
        state.Version = 1;
        state.RefreshedAt = ctx.Timestamp;
        ctx.Db.MaterializedState.Id.Update(state);
    }
}
