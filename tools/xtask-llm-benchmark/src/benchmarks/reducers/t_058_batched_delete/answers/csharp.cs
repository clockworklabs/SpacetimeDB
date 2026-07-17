using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "WorkItem", Public = true)]
    public partial struct WorkItem { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong GroupId; }

    [Table(Accessor = "DeleteJob", Scheduled = nameof(RunDeleteBatch), ScheduledAt = nameof(DeleteJob.ScheduledAt))]
    public partial struct DeleteJob
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong GroupId;
    }

    private static void Enqueue(ReducerContext ctx, ulong groupId) => ctx.Db.DeleteJob.Insert(new DeleteJob
    {
        ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration { Microseconds = 1_000 }),
        GroupId = groupId,
    });

    [Reducer]
    public static void SeedGroup(ReducerContext ctx, ulong groupId, uint count)
    {
        for (uint offset = 0; offset < count; offset++) ctx.Db.WorkItem.Insert(new WorkItem { Id = groupId * 100 + offset, GroupId = groupId });
    }

    [Reducer]
    public static void RequestDelete(ReducerContext ctx, ulong groupId) => Enqueue(ctx, groupId);

    [Reducer]
    public static void RunDeleteBatch(ReducerContext ctx, DeleteJob job)
    {
        foreach (var row in ctx.Db.WorkItem.GroupId.Filter(job.GroupId).Take(2).ToList()) ctx.Db.WorkItem.Id.Delete(row.Id);
        if (ctx.Db.WorkItem.GroupId.Filter(job.GroupId).Any()) Enqueue(ctx, job.GroupId);
    }
}
