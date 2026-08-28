import { ConnectionId } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';

const presenceSession = table({ name: 'presence_session', public: true }, {
  connectionId: t.connectionId().primaryKey(), identity: t.identity().index('btree'), connectedAt: t.timestamp(),
});
const spacetimedb = schema({ presenceSession });
export default spacetimedb;
type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;

function addSession(ctx: Ctx, connectionId: ConnectionId) {
  ctx.db.presenceSession.insert({ connectionId, identity: ctx.sender, connectedAt: ctx.timestamp });
}
function removeSession(ctx: Ctx, connectionId: ConnectionId) { ctx.db.presenceSession.connectionId.delete(connectionId); }

export const clientConnected = spacetimedb.clientConnected(ctx => {
  if (!ctx.connectionId) throw new Error('connection id missing');
  addSession(ctx, ctx.connectionId);
});
export const clientDisconnected = spacetimedb.clientDisconnected(ctx => {
  if (!ctx.connectionId) throw new Error('connection id missing');
  removeSession(ctx, ctx.connectionId);
});
export const exercise_presence = spacetimedb.reducer(ctx => {
  const first = new ConnectionId(1n);
  const second = new ConnectionId(2n);
  addSession(ctx, first);
  addSession(ctx, second);
  removeSession(ctx, first);
});
