import { table, schema, t } from 'spacetimedb/server';

export const Event = table({
  name: 'event',
}, {
  id: t.u64().primaryKey().autoInc(),
  kind: t.string(),
});

const spacetimedb = schema(Event);

spacetimedb.clientConnected(ctx => {
  ctx.db.event.insert({ id: 0n, kind: "connected" });
});

spacetimedb.clientDisconnected(ctx => {
  ctx.db.event.insert({ id: 0n, kind: "disconnected" });
});
