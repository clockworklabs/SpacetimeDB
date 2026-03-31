import { table, schema, t } from 'spacetimedb/server';

const event = table(
  {
    name: 'event',
  },
  {
    id: t.u64().primaryKey().autoInc(),
    kind: t.string(),
  }
);

const spacetimedb = schema({ event });
export default spacetimedb;

export const onConnect = spacetimedb.clientConnected(ctx => {
  ctx.db.event.insert({ id: 0n, kind: 'connected' });
});

export const onDisconnect = spacetimedb.clientDisconnected(ctx => {
  ctx.db.event.insert({ id: 0n, kind: 'disconnected' });
});
