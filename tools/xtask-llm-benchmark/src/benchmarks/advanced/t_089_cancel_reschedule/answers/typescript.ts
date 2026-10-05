import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
const timedReservation = table(
  { name: 'timed_reservation', public: true },
  {
    id: t.u64().primaryKey(),
    generation: t.u64(),
    status: t.string(),
  }
);
const expiryJob = table(
  { name: 'expiry_job', scheduled: (): any => expire_reservation },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    reservationId: t.u64(),
    generation: t.u64(),
  }
);
const expiryResult = table(
  { name: 'expiry_result', public: true },
  {
    scheduledId: t.u64().primaryKey(),
    reservationId: t.u64(),
    generation: t.u64(),
    applied: t.bool(),
  }
);
const spacetimedb = schema({ timedReservation, expiryJob, expiryResult });
export default spacetimedb;

export const renew = spacetimedb.reducer(
  { id: t.u64(), delayMs: t.u64() },
  (ctx, { id, delayMs }) => {
    if (delayMs === 0n || delayMs > 60000n)
      throw new SenderError('invalid delay');
    const generation =
      (ctx.db.timedReservation.id.find(id)?.generation ?? 0n) + 1n;
    const next = { id, generation, status: 'active' };
    if (ctx.db.timedReservation.id.find(id))
      ctx.db.timedReservation.id.update(next);
    else ctx.db.timedReservation.insert(next);
    ctx.db.expiryJob.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(
        ctx.timestamp.microsSinceUnixEpoch + delayMs * 1000n
      ),
      reservationId: id,
      generation,
    });
  }
);

export const cancel = spacetimedb.reducer({ id: t.u64() }, (ctx, { id }) => {
  const row = ctx.db.timedReservation.id.find(id);
  if (row && row.status !== 'cancelled')
    ctx.db.timedReservation.id.update({
      ...row,
      generation: row.generation + 1n,
      status: 'cancelled',
    });
});

export const expire_reservation = spacetimedb.reducer(
  { job: expiryJob.rowType },
  (ctx, { job }) => {
    if (!ctx.sender.isEqual(ctx.databaseIdentity))
      throw new SenderError('scheduler only');
    if (ctx.db.expiryResult.scheduledId.find(job.scheduledId)) return;
    const row = ctx.db.timedReservation.id.find(job.reservationId);
    const applied =
      !!row && row.status === 'active' && row.generation === job.generation;
    if (applied)
      ctx.db.timedReservation.id.update({ ...row!, status: 'expired' });
    ctx.db.expiryResult.insert({
      scheduledId: job.scheduledId,
      reservationId: job.reservationId,
      generation: job.generation,
      applied,
    });
  }
);
