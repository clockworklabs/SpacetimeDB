import { schema, table, t } from 'spacetimedb/server';

export const spacetimedb = schema(
  table(
    { name: 'onlineUsers', public: true },
    {
      identity: t.identity().primaryKey(),
    },
  ),
  table(
    { name: 'messages', public: true },
    {
      id: t.u64().primaryKey().autoInc(),
      sender_identity: t.identity(),
      content: t.string(),
      timestamp: t.timestamp(),
    },
  ),
);

spacetimedb.clientConnected((ctx) => {
  ctx.db.onlineUsers.insert({ identity: ctx.sender });
});

spacetimedb.clientDisconnected((ctx) => {
  ctx.db.onlineUsers.identity.delete(ctx.sender);
});

spacetimedb.reducer('send_message', { content: t.string() }, (ctx, { content }) => {
  ctx.db.messages.insert({
    id: 0n,
    sender_identity: ctx.sender,
    timestamp: ctx.timestamp,
    content,
  });
});
