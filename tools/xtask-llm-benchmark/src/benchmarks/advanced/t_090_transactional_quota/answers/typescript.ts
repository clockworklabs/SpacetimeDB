import { schema, table, t, SenderError } from 'spacetimedb/server';

const quota = table(
  { name: 'quota', public: true },
  {
    owner: t.identity().primaryKey(),
    used: t.u64(),
  }
);
const acceptedRequest = table(
  { name: 'accepted_request', public: true },
  {
    requestId: t.string().primaryKey(),
    owner: t.identity(),
    units: t.u64(),
  }
);
const spacetimedb = schema({ quota, acceptedRequest });
export default spacetimedb;

export const submit = spacetimedb.reducer(
  { requestId: t.string(), units: t.i64() },
  (ctx, { requestId, units }) => {
    const owner = ctx.sender,
      old = ctx.db.acceptedRequest.requestId.find(requestId);
    if (old) {
      if (old.owner.isEqual(owner) && old.units === units) return;
      throw new SenderError('request conflict');
    }
    if (!requestId || units <= 0n) throw new SenderError('invalid request');
    const used = ctx.db.quota.owner.find(owner)?.used ?? 0n;
    if (units > 3n - used) throw new SenderError('quota exceeded');
    const next = { owner, used: used + units };
    if (ctx.db.quota.owner.find(owner)) ctx.db.quota.owner.update(next);
    else ctx.db.quota.insert(next);
    ctx.db.acceptedRequest.insert({ requestId, owner, units });
  }
);
