import { schema, table, t, SenderError } from 'spacetimedb/server';

const leasedJob = table(
  { name: 'leased_job', public: true },
  {
    id: t.u64().primaryKey(),
    worker: t.string(),
    generation: t.u64(),
    expiresAt: t.i64(),
    done: t.bool(),
    result: t.string(),
  }
);
const spacetimedb = schema({ leasedJob });
export default spacetimedb;

export const create_job = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    ctx.db.leasedJob.insert({
      id,
      worker: '',
      generation: 0n,
      expiresAt: 0n,
      done: false,
      result: '',
    });
  }
);

export const claim = spacetimedb.reducer(
  { id: t.u64(), worker: t.string(), leaseMs: t.u64() },
  (ctx, { id, worker, leaseMs }) => {
    if (!worker || leaseMs === 0n || leaseMs > 60000n)
      throw new SenderError('invalid lease');
    const now = ctx.timestamp.microsSinceUnixEpoch;
    const row = ctx.db.leasedJob.id.find(id);
    if (!row) throw new SenderError('missing job');
    if (row.done) throw new SenderError('already done');
    if (row.worker && now < row.expiresAt) throw new SenderError('lease busy');
    ctx.db.leasedJob.id.update({
      ...row,
      worker,
      generation: row.generation + 1n,
      expiresAt: now + leaseMs * 1000n,
    });
  }
);

export const complete = spacetimedb.reducer(
  { id: t.u64(), worker: t.string(), generation: t.u64(), result: t.string() },
  (ctx, { id, worker, generation, result }) => {
    const row = ctx.db.leasedJob.id.find(id);
    if (!row) throw new SenderError('missing job');
    if (row.done) {
      if (
        row.worker === worker &&
        row.generation === generation &&
        row.result === result
      )
        return;
      throw new SenderError('already done');
    }
    if (
      !worker ||
      generation === 0n ||
      row.worker !== worker ||
      row.generation !== generation ||
      ctx.timestamp.microsSinceUnixEpoch >= row.expiresAt
    )
      throw new SenderError('stale lease');
    ctx.db.leasedJob.id.update({ ...row, done: true, result });
  }
);
