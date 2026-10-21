using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "TimedReservation", Public = true)]
    public partial struct TimedReservation
    {
        [PrimaryKey]
        public ulong Id;
        public ulong Generation;
        public string Status;
    }

    [Table(
        Accessor = "ExpiryJob",
        Scheduled = nameof(ExpireReservation),
        ScheduledAt = nameof(ExpiryJob.ScheduledAt)
    )]
    public partial struct ExpiryJob
    {
        [PrimaryKey, AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public ulong ReservationId;
        public ulong Generation;
    }

    [Table(Accessor = "ExpiryResult", Public = true)]
    public partial struct ExpiryResult
    {
        [PrimaryKey]
        public ulong ScheduledId;
        public ulong ReservationId;
        public ulong Generation;
        public bool Applied;
    }

    [Reducer]
    public static void Renew(ReducerContext ctx, ulong id, ulong delayMs)
    {
        if (delayMs == 0 || delayMs > 60000)
            throw new Exception("invalid delay");
        var generation = (ctx.Db.TimedReservation.Id.Find(id)?.Generation ?? 0) + 1;
        var next = new TimedReservation
        {
            Id = id,
            Generation = generation,
            Status = "active",
        };
        if (ctx.Db.TimedReservation.Id.Find(id) is null)
            ctx.Db.TimedReservation.Insert(next);
        else
            ctx.Db.TimedReservation.Id.Update(next);
        ctx.Db.ExpiryJob.Insert(
            new ExpiryJob
            {
                ScheduledAt = new ScheduleAt.Time(
                    ctx.Timestamp + new TimeDuration { Microseconds = (long)delayMs * 1000 }
                ),
                ReservationId = id,
                Generation = generation,
            }
        );
    }

    [Reducer]
    public static void Cancel(ReducerContext ctx, ulong id)
    {
        if (
            ctx.Db.TimedReservation.Id.Find(id) is TimedReservation row
            && row.Status != "cancelled"
        )
        {
            row.Generation++;
            row.Status = "cancelled";
            ctx.Db.TimedReservation.Id.Update(row);
        }
    }

    [Reducer]
    public static void ExpireReservation(ReducerContext ctx, ExpiryJob job)
    {
        if (ctx.Sender != ctx.DatabaseIdentity)
            throw new Exception("scheduler only");
        if (ctx.Db.ExpiryResult.ScheduledId.Find(job.ScheduledId) is not null)
            return;
        var applied = false;
        if (
            ctx.Db.TimedReservation.Id.Find(job.ReservationId) is TimedReservation row
            && row.Status == "active"
            && row.Generation == job.Generation
        )
        {
            row.Status = "expired";
            ctx.Db.TimedReservation.Id.Update(row);
            applied = true;
        }
        ctx.Db.ExpiryResult.Insert(
            new ExpiryResult
            {
                ScheduledId = job.ScheduledId,
                ReservationId = job.ReservationId,
                Generation = job.Generation,
                Applied = applied,
            }
        );
    }
}
