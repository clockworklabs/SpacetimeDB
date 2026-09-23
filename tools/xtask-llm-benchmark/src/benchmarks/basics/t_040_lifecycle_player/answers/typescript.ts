import { schema, table, t } from 'spacetimedb/server';

const online_player = table({
  name: 'online_player',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  connectedAt: t.timestamp(),
});

const spacetimedb = schema({ online_player });
export default spacetimedb;

export const clientConnected = spacetimedb.clientConnected(ctx => {
  ctx.db.online_player.insert({
    identity: ctx.sender,
    connectedAt: ctx.timestamp,
  });
});

export const clientDisconnected = spacetimedb.clientDisconnected(ctx => {
  ctx.db.online_player.identity.delete(ctx.sender);
});
