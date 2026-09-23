using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "ProcedureResult", Public = true)]
    public partial struct ProcedureResult { [PrimaryKey] public ulong Id; public uint Value; }

    [Table(Accessor = "ProcedureJob", Scheduled = nameof(RunScheduledProcedure), ScheduledAt = nameof(ProcedureJob.ScheduledAt))]
    public partial struct ProcedureJob
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong Id;
        public uint Lhs;
        public uint Rhs;
    }

    [Reducer]
    public static void ScheduleProcedure(ReducerContext ctx, ulong id, uint lhs, uint rhs) =>
        ctx.Db.ProcedureJob.Insert(new ProcedureJob {
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration { Microseconds = 1_000 }), Id = id, Lhs = lhs, Rhs = rhs,
        });

    [SpacetimeDB.Procedure]
    public static void RunScheduledProcedure(ProcedureContext ctx, ProcedureJob job) =>
        ctx.WithTx(tx => { tx.Db.ProcedureResult.Insert(new ProcedureResult { Id = job.Id, Value = job.Lhs + job.Rhs }); return 0; });
}
