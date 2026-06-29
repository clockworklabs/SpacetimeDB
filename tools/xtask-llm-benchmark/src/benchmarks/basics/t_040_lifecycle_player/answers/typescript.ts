import { schema, table, t } from 'spacetimedb/server';

const onlinePlayer = table({
  name: 'online_player',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  connectedAt: t.timestamp(),
});

const spacetimedb = schema({ onlinePlayer });
export default spacetimedb;

export const clientConnected = spacetimedb.clientConnected(ctx => {
  ctx.db.onlinePlayer.insert({
    identity: ctx.sender,
    connectedAt: ctx.timestamp,
  });
});

export const clientDisconnected = spacetimedb.clientDisconnected(ctx => {
  ctx.db.onlinePlayer.identity.delete(ctx.sender);
});
