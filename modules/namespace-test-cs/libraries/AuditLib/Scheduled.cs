using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

namespace AuditLib;

[Table(
    Accessor = "ReducerJob",
    Name = "reducer_jobs",
    Scheduled = nameof(ScheduledFunctions.ReducerTick),
    ScheduledAt = nameof(Due)
)]
public partial struct ReducerJob
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public uint JobId;
    public ScheduleAt Due;
    public uint Payload;
}

[Table(
    Accessor = "ProcedureJob",
    Name = "procedure_jobs",
    Scheduled = nameof(ScheduledFunctions.ProcedureTick),
    ScheduledAt = nameof(Due)
)]
public partial struct ProcedureJob
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public uint JobId;
    public uint Payload;
    public ScheduleAt Due;
}

[Table(Accessor = "ScheduleResult", Public = true)]
public partial struct ScheduleResult
{
    [PrimaryKey]
    public uint JobId;
    public uint Payload;
    public string Kind;
    public uint Executions;
    public ulong ScheduledId;
}

public static partial class ScheduledFunctions
{
    private const uint PayloadBase = 2000;

    [Reducer]
    public static void StartSchedules(ReducerContext ctx)
    {
        var due = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration(100_000));
        ctx.Db.ReducerJob.Insert(
            new ReducerJob
            {
                JobId = 1,
                Due = due,
                Payload = PayloadBase + 1,
            }
        );
        ctx.Db.ProcedureJob.Insert(
            new ProcedureJob
            {
                JobId = 2,
                Due = due,
                Payload = PayloadBase + 2,
            }
        );
        VolatileNonatomicScheduleImmediateReducerTick(
            new ReducerJob
            {
                JobId = 3,
                Due = due,
                Payload = PayloadBase + 3,
            }
        );
        VolatileNonatomicScheduleImmediateProcedureTick(
            new ProcedureJob
            {
                JobId = 4,
                Due = due,
                Payload = PayloadBase + 4,
            }
        );
        ctx.Db.ReducerJob.Insert(
            new ReducerJob
            {
                JobId = 5,
                Due = new ScheduleAt.Interval(new TimeDuration(100_000)),
                Payload = PayloadBase + 5,
            }
        );
    }

    [Reducer(Name = "run_reducer_job")]
    public static void ReducerTick(ReducerContext ctx, ReducerJob job)
    {
        var previous = ctx.Db.ScheduleResult.JobId.Find(job.JobId);
        var result = new ScheduleResult
        {
            JobId = job.JobId,
            Payload = job.Payload,
            Kind = "reducer",
            Executions = (previous?.Executions ?? 0) + 1,
            ScheduledId = job.Id,
        };
        if (previous is null)
            ctx.Db.ScheduleResult.Insert(result);
        else
            ctx.Db.ScheduleResult.JobId.Update(result);
    }

    [Procedure(Name = "run_procedure_job")]
    public static void ProcedureTick(ProcedureContext ctx, ProcedureJob job)
    {
        ctx.WithTx(tx =>
        {
            var previous = tx.Db.ScheduleResult.JobId.Find(job.JobId);
            var result = new ScheduleResult
            {
                JobId = job.JobId,
                Payload = job.Payload,
                Kind = "procedure",
                Executions = (previous?.Executions ?? 0) + 1,
                ScheduledId = job.Id,
            };
            if (previous is null)
                tx.Db.ScheduleResult.Insert(result);
            else
                tx.Db.ScheduleResult.JobId.Update(result);
            return 0;
        });
    }

    [Reducer]
    public static void CancelSchedules(ReducerContext ctx)
    {
        var repeat = ctx.Db.ScheduleResult.JobId.Find(5)!.Value;
        if (repeat.Executions < 2)
            throw new Exception("Repeating schedule has not run twice.");
        ctx.Db.ReducerJob.Id.Delete(repeat.ScheduledId);
        ctx.Db.ScheduleResult.Insert(
            new ScheduleResult
            {
                JobId = 6,
                Payload = PayloadBase + 6,
                Kind = "cancel",
                Executions = repeat.Executions,
            }
        );
        // This later job provides a scheduler-driven observation point after cancellation.
        ctx.Db.ReducerJob.Insert(
            new ReducerJob
            {
                JobId = 7,
                Due = new ScheduleAt.Time(ctx.Timestamp + new TimeDuration(500_000)),
                Payload = PayloadBase + 7,
            }
        );
    }
}
