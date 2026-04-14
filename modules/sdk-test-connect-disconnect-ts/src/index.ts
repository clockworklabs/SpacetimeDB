// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { schema, t, table } from 'spacetimedb/server';

const connected = table(
  { name: 'connected', public: true },
  { identity: t.identity() }
);

const disconnected = table(
  { name: 'disconnected', public: true },
  { identity: t.identity() }
);

const spacetimedb = schema({ connected, disconnected });
export default spacetimedb;

export const identity_connected = spacetimedb.clientConnected(ctx => {
  ctx.db.connected.insert({ identity: ctx.sender });
});

export const identity_disconnected = spacetimedb.clientDisconnected(ctx => {
  ctx.db.disconnected.insert({ identity: ctx.sender });
});
