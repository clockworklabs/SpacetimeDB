using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "QueuedWork", Public = true)]
    public partial struct QueuedWork
    {
        [PrimaryKey]
        public ulong Id;
        public long Input;
        public string Status;
        public ulong Attempts;
    }

    [Table(Accessor = "WorkEffect", Public = true)]
    public partial struct WorkEffect
    {
        [PrimaryKey]
        public ulong Id;
        public long Value;
    }

    [Table(
        Accessor = "WorkTimer",
        Scheduled = nameof(ExecuteWork),
        ScheduledAt = nameof(WorkTimer.ScheduledAt)
    )]
    public partial struct WorkTimer
    {
        [PrimaryKey, AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong WorkId;
    }

    [Reducer]
    public static void Enqueue(ReducerContext ctx, ulong id, long input)
    {
        if (ctx.Db.QueuedWork.Id.Find(id) is QueuedWork old)
        {
            if (old.Input == input)
                return;
            throw new Exception("request conflict");
        }
        if (input < -1000000 || input > 1000000)
            throw new Exception("invalid input");
        ctx.Db.QueuedWork.Insert(
            new QueuedWork
            {
                Id = id,
                Input = input,
                Status = "queued",
                Attempts = 0,
            }
        );
        ctx.Db.WorkTimer.Insert(
            new WorkTimer
            {
                ScheduledAt = new ScheduleAt.Time(
                    ctx.Timestamp + new TimeDuration { Microseconds = 1000 }
                ),
                WorkId = id,
            }
        );
    }

    [Reducer]
    public static void ExecuteWork(ReducerContext ctx, WorkTimer job)
    {
        if (ctx.Sender != ctx.DatabaseIdentity)
            throw new Exception("scheduler only");
        if (ctx.Db.QueuedWork.Id.Find(job.WorkId) is QueuedWork row && row.Status == "queued")
        {
            row.Attempts++;
            if (row.Input < 0)
                row.Status = "failed";
            else
            {
                ctx.Db.WorkEffect.Insert(new WorkEffect { Id = row.Id, Value = row.Input * 2 });
                row.Status = "complete";
            }
            ctx.Db.QueuedWork.Id.Update(row);
        }
    }
}
