import { schema, table, t } from 'spacetimedb/server';
import gw from './remote_bindings/game_world';

const outboundPing = table(
  { outbox: { receiver: gw.reducers.receivePing } },
  {
    id: t.u64().primaryKey().autoInc(),
    target: gw.identity().target(),
    body: t.string(),
  }
);
const outboundDetailedPing = gw.outbox.receiveDetailedPing({
  name: 'outbound_detailed_ping',
});
const spacetimedb = schema({
  outboundPing,
  outboundDetailedPing,
  pingResult: table(
    {},
    {
      id: t.u64().primaryKey(),
      callbackOrder: t.u64().autoInc(),
      moduleVersion: t.u32(),
      body: t.string(),
      succeeded: t.bool(),
      error: t.option(t.string()),
    }
  ),
  detailedPingResult: table(
    {},
    {
      id: t.u64().primaryKey(),
      body: t.string(),
      details: gw.reducers.receiveDetailedPing.params.details,
    }
  ),
});
export default spacetimedb;
export const enqueuePing = spacetimedb.reducer(
  { target: gw.identity(), body: t.string() },
  (ctx, args) => {
    ctx.db.outboundPing.insert({ id: 0n, ...args });
  }
);
export const enqueuePings = spacetimedb.reducer(
  { target: gw.identity(), bodies: t.array(t.string()) },
  (ctx, { target, bodies }) => {
    for (const body of bodies)
      ctx.db.outboundPing.insert({ id: 0n, target, body });
  }
);
export const enqueueDetailedPing = spacetimedb.reducer(
  { target: gw.identity(), body: t.string() },
  (ctx, { target, body }) => {
    ctx.db.outboundDetailedPing.insert({
      msgId: 0n,
      target,
      body,
      details: {
        labels: ['nested', 'idc'],
        checkpoints: [
          { sequence: 1, state: { tag: 'Ready' } },
          { sequence: 2, state: { tag: 'Deferred', value: 'waiting' } },
          { sequence: 3, state: { tag: 'Deferred', value: undefined } },
        ],
        note: 'optional-note',
      },
    });
  }
);
export const onPingResult = spacetimedb.reducer(
  { reactive: { onResult: outboundPing } },
  { row: t.rowOf(outboundPing), result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) =>
    ctx.db.pingResult.insert({
      id: row.id,
      callbackOrder: 0n,
      moduleVersion: 1,
      body: row.body,
      succeeded: 'ok' in result,
      error: 'err' in result ? result.err : undefined,
    })
);
export const onDetailedPingResult = spacetimedb.reducer(
  { reactive: { onResult: outboundDetailedPing } },
  {
    row: t.rowOf(outboundDetailedPing),
    result: t.result(t.unit(), t.string()),
  },
  (ctx, { row, result }) => {
    if ('err' in result) throw new Error(result.err);
    ctx.db.detailedPingResult.insert({
      id: row.msgId,
      body: row.body,
      details: row.details,
    });
  }
);
