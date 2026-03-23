import { schema, table, t } from 'spacetimedb/server';

const onlinePlayer = table({
  name: 'onlinePlayer',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  connectedAt: t.timestamp(),
});

const spacetimedb = schema({ onlinePlayer });
export default spacetimedb;

export const onConnect = spacetimedb.clientConnected(ctx => {
  ctx.db.onlinePlayer.insert({
    identity: ctx.sender,
    connectedAt: ctx.timestamp,
  });
});

export const onDisconnect = spacetimedb.clientDisconnected(ctx => {
  ctx.db.onlinePlayer.identity.delete(ctx.sender);
});
