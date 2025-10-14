// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { schema, t, table } from 'spacetimedb/server';

const Connected = table(
  { name: 'connected', public: true },
  { identity: t.identity() }
);

const Disconnected = table(
  { name: 'disconnected', public: true },
  { identity: t.identity() }
);

const spacetimedb = schema(Connected, Disconnected);

spacetimedb.reducer('identity_connected', ctx => {
  ctx.db.connected.insert({ identity: ctx.sender });
});

spacetimedb.reducer('identity_disconnected', ctx => {
  ctx.db.disconnected.insert({ identity: ctx.sender });
});
