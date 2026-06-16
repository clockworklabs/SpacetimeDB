import { schema, table, t } from 'spacetimedb/server';

const rate_limit = table({
  name: 'rate_limit',
}, {
  identity: t.identity().primaryKey(),
  last_call_us: t.u64(),
});

const action_log = table({
  name: 'action_log',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  identity: t.identity(),
  payload: t.string(),
});

const spacetimedb = schema({ rate_limit, action_log });
export default spacetimedb;

export const limited_action = spacetimedb.reducer(
  { payload: t.string() },
  (ctx, { payload }) => {
    const now = BigInt(ctx.timestamp.microsSinceUnixEpoch);
    const entry = ctx.db.rate_limit.identity.find(ctx.sender);
    if (entry) {
      if (now - entry.last_call_us < 1_000_000n) throw new Error('rate limited');
      ctx.db.rate_limit.identity.update({ ...entry, last_call_us: now });
    } else {
      ctx.db.rate_limit.insert({ identity: ctx.sender, last_call_us: now });
    }
    ctx.db.action_log.insert({ id: 0n, identity: ctx.sender, payload });
  }
);
