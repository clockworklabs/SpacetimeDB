// Checks that `IsInternal` is true exactly when the sender is this database;
// the C# counterpart of `is-internal-test`.
namespace SpacetimeDB.IsInternalTest;

using SpacetimeDB;

[Table(Accessor = "jobs", Scheduled = nameof(Module.scheduled), ScheduledAt = nameof(scheduled_at))]
public partial struct Job
{
    [PrimaryKey]
    [AutoInc]
    public ulong id;
    public ScheduleAt scheduled_at;
}

[Table(Accessor = "finished")]
public partial struct Finished
{
    [PrimaryKey]
    public ulong id;
}

static partial class Module
{
    static void Check(bool condition, string message)
    {
        if (!condition)
        {
            throw new Exception(message);
        }
    }

    [Reducer(ReducerKind.Init)]
    public static void init(ReducerContext ctx)
    {
        // The sender of `init` is the database's owner, so it is not internal.
        Check(ctx.Sender != ctx.DatabaseIdentity, "init sender is the database");
        Check(!ctx.SenderAuth.IsInternal, "init is internal");
    }

    [Reducer]
    public static void check(ReducerContext ctx)
    {
        Check(
            ctx.SenderAuth.IsInternal == (ctx.Sender == ctx.DatabaseIdentity),
            "IsInternal differs from whether the sender is the database"
        );
    }

    [Procedure]
    public static bool check_procedure(ProcedureContext ctx)
    {
        var sender = ctx.Sender;
        var isSelf = sender == ProcedureContext.Identity;
        Check(
            ctx.SenderAuth.IsInternal == isSelf,
            "IsInternal differs from whether the sender is the database"
        );
        ctx.WithTx(tx =>
        {
            Check(tx.Sender == sender, "transaction sender differs");
            Check(
                tx.SenderAuth.IsInternal == isSelf,
                "transaction IsInternal differs from whether the sender is the database"
            );
            return 0;
        });
        return true;
    }

    [Reducer]
    public static void schedule(ReducerContext ctx)
    {
        ctx.Db.jobs.Insert(new Job { id = 0, scheduled_at = new ScheduleAt.Time(ctx.Timestamp) });
    }

    [Reducer]
    public static void scheduled(ReducerContext ctx, Job job)
    {
        // The database is the sender of its scheduled reducers.
        Check(ctx.Sender == ctx.DatabaseIdentity, "scheduled reducer sender is not the database");
        Check(ctx.SenderAuth.IsInternal, "scheduled reducer is not internal");
        Check(!ctx.SenderAuth.HasJwt, "scheduled reducer has a JWT");
        ctx.Db.finished.Insert(new Finished { id = job.id });
    }

    [Procedure]
    public static bool scheduled_finished(ProcedureContext ctx) =>
        ctx.WithTx(tx => tx.Db.finished.Count > 0);
}
