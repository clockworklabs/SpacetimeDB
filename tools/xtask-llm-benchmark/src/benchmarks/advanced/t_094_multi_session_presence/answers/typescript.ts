import { schema, table, t, SenderError } from 'spacetimedb/server';

const liveSession = table(
  { name: 'live_session', public: true },
  {
    connectionId: t.connectionId().primaryKey(),
    owner: t.identity(),
  }
);
const onlineUser = table(
  { name: 'online_user', public: true },
  {
    owner: t.identity().primaryKey(),
    connections: t.u64(),
  }
);
const spacetimedb = schema({ liveSession, onlineUser });
export default spacetimedb;

export const clientConnected = spacetimedb.clientConnected(ctx => {
  const connectionId = ctx.connectionId;
  if (!connectionId) throw new SenderError('connection missing');
  const owner = ctx.sender;
  ctx.db.liveSession.insert({ connectionId, owner });
  const row = ctx.db.onlineUser.owner.find(owner);
  if (row)
    ctx.db.onlineUser.owner.update({
      ...row,
      connections: row.connections + 1n,
    });
  else ctx.db.onlineUser.insert({ owner, connections: 1n });
});
export const clientDisconnected = spacetimedb.clientDisconnected(ctx => {
  const connectionId = ctx.connectionId;
  if (!connectionId) throw new SenderError('connection missing');
  const session = ctx.db.liveSession.connectionId.find(connectionId);
  if (!session) return;
  ctx.db.liveSession.connectionId.delete(connectionId);
  const row = ctx.db.onlineUser.owner.find(session.owner);
  if (row) {
    if (row.connections === 1n) ctx.db.onlineUser.owner.delete(session.owner);
    else
      ctx.db.onlineUser.owner.update({
        ...row,
        connections: row.connections - 1n,
      });
  }
});
