// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { schema, t, table } from 'spacetimedb/server';

const Connected = table(
  { accessor: 'connected', public: true },
  { identity: t.identity() }
);

const Disconnected = table(
  { accessor: 'disconnected', public: true },
  { identity: t.identity() }
);

const spacetimedb = schema(Connected, Disconnected);

spacetimedb.clientConnected('identity_connected', ctx => {
  ctx.db.connected.insert({ identity: ctx.sender });
});

spacetimedb.clientDisconnected('identity_disconnected', ctx => {
  ctx.db.disconnected.insert({ identity: ctx.sender });
});
