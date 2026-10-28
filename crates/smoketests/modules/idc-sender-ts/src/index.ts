import { schema, table, t } from 'spacetimedb/server';
import gw from './remote_bindings/game_world';
import auth from './remote_bindings/auth';

const outboundPing = gw.outbox.receivePing();
const outboundDetailedPing = table(
  { outbox: { receiver: gw.reducers.receiveDetailedPing } },
  {
    msgId: t.u64().primaryKey().autoInc(),
    target: gw.identity().target(),
    body: t.string(),
    details: gw.reducers.receiveDetailedPing.params.details,
  }
);
const outboundAuthDetails = table(
  { outbox: { receiver: auth.reducers.receiveDetailedPing } },
  {
    msgId: t.u64().primaryKey().autoInc(),
    target: auth.identity().target(),
    body: t.string(),
    details: auth.reducers.receiveDetailedPing.params.details,
  }
);
const spacetimedb = schema({
  outboundPing,
  outboundDetailedPing,
  outboundAuthDetails,
  senderDetails: table(
    {},
    {
      id: t.u64().primaryKey(),
      // This source type name is also present in both receiver dependencies.
      details: t.object('PingDetails', { source: t.string() }),
    }
  ),
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
export const enqueueTwoDependencies = spacetimedb.reducer(
  { world: gw.identity(), authTarget: auth.identity() },
  (ctx, { world, authTarget }) => {
    ctx.db.outboundDetailedPing.insert({
      msgId: 0n,
      target: world,
      body: 'world-dependency',
      details: { labels: ['world'], checkpoints: [], note: undefined },
    });
    ctx.db.outboundAuthDetails.insert({
      msgId: 0n,
      target: authTarget,
      body: 'auth-dependency',
      details: { labels: ['auth'], checkpoints: [], note: undefined },
    });
    ctx.db.senderDetails.insert({ id: 1n, details: { source: 'sender' } });
  }
);
export const enqueuePing = spacetimedb.reducer(
  { target: gw.identity(), body: t.string() },
  (ctx, { target, body }) => {
    ctx.db.outboundPing.insert({ __msgId: 0n, __target: target, body });
  }
);
export const enqueuePings = spacetimedb.reducer(
  { target: gw.identity(), bodies: t.array(t.string()) },
  (ctx, { target, bodies }) => {
    for (const body of bodies)
      ctx.db.outboundPing.insert({ __msgId: 0n, __target: target, body });
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
  { row: outboundPing.rowType, result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) =>
    ctx.db.pingResult.insert({
      id: row.__msgId,
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
    row: outboundDetailedPing.rowType,
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
