import {
  schema,
  table,
  t,
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { errors } from '../errors.js';
import {
  DEFAULT_PRESENCE_STATUS,
  removePresence,
  runPresenceSweep,
  updatePresenceConfig,
  upsertPresence,
} from '../presence.js';
import { presenceConfigRow, presenceEntryRow } from '../tables.js';

export const presenceEntry = table(
  { name: 'presence_entry', public: false },
  presenceEntryRow
);

const presenceConfig = table(
  { name: 'presence_config', public: false },
  presenceConfigRow
);

const presenceAdminIdentity = table(
  { name: 'presence_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

const presenceSweepTick = table(
  { name: 'presence_sweep_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({
  presenceEntry,
  presenceConfig,
  presenceAdminIdentity,
  presenceSweepTick,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type Tx = ReducerCtx<Schema>;

/** The scope the client-callable operations write. Hosts choose their own scopes. */
export const GLOBAL_SCOPE = 'presence.global';
const MAX_VIEW_ROWS = 1000;

const heartbeatResult = t.object('PresenceHeartbeatResult', {
  scope: t.string(),
  subject: t.string(),
  status: t.string(),
  expiresAt: t.timestamp(),
});

function takeRows<T>(rows: Iterable<T>): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= MAX_VIEW_ROWS) break;
    out.push(row);
  }
  return out;
}

function isAdmin(ctx: Tx | ViewCtx<Schema>): boolean {
  return ctx.db.presenceAdminIdentity.identity.find(ctx.sender) != null;
}

function requireAdmin(ctx: Tx): void {
  if (!isAdmin(ctx)) throw new SenderError(errors.notAuthorized);
}

function expiredRows(tx: Tx) {
  return tx.db.presenceEntry.expiresAt.filter(
    new Range(undefined, { tag: 'included', value: tx.timestamp })
  );
}

/** Records the caller's global presence. The subject is the caller's identity. */
export const heartbeat = spacetimedb.procedure(
  {
    status: t.option(t.string()),
    activity: t.option(t.string()),
    payloadJson: t.option(t.string()),
    ttlSeconds: t.option(t.u32()),
  },
  heartbeatResult,
  (ctx, args) =>
    ctx.withTx(tx => {
      const row = upsertPresence(tx, {
        scope: GLOBAL_SCOPE,
        subject: ctx.sender.toHexString(),
        status: args.status ?? DEFAULT_PRESENCE_STATUS,
        activity: args.activity,
        payloadJson: args.payloadJson,
        ttlSeconds: args.ttlSeconds,
      });
      return {
        scope: row.scope,
        subject: row.subject,
        status: row.status,
        expiresAt: row.expiresAt,
      };
    })
);

export const clearPresence = spacetimedb.reducer({}, ctx => {
  removePresence(ctx, GLOBAL_SCOPE, ctx.sender.toHexString());
});

export const runSweep = spacetimedb.procedure({}, t.u32(), ctx =>
  ctx.withTx(tx => {
    requireAdmin(tx);
    return runPresenceSweep(tx, expiredRows(tx));
  })
);

export const addPresenceAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    if (ctx.db.presenceAdminIdentity.identity.find(args.identity) == null) {
      ctx.db.presenceAdminIdentity.insert({
        identity: args.identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const removePresenceAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    const row = ctx.db.presenceAdminIdentity.identity.find(args.identity);
    if (!row) return;
    if (ctx.db.presenceAdminIdentity.count() <= 1n) {
      throw new SenderError(errors.cannotRemoveLastAdmin);
    }
    ctx.db.presenceAdminIdentity.delete(row);
  }
);

export const updateConfig = spacetimedb.reducer(
  { defaultTtlSeconds: t.u32(), sweepBatch: t.u32() },
  (ctx, args) => {
    requireAdmin(ctx);
    updatePresenceConfig(ctx, args);
  }
);

/** Up to 1,000 global presence entries, including their activity and payload. */
export const presenceOnline = spacetimedb.view(
  { name: 'presence_online', public: true },
  t.array(presenceEntry.rowType),
  ctx => takeRows(ctx.db.presenceEntry.scope.filter(GLOBAL_SCOPE))
);

export const presenceEntriesAdmin = spacetimedb.view(
  { name: 'presence_entries_admin', public: true },
  t.array(presenceEntry.rowType),
  ctx => (isAdmin(ctx) ? takeRows(ctx.db.presenceEntry.iter()) : [])
);

export const presenceSweep = spacetimedb.reducer(
  { onSchedule: presenceSweepTick },
  { arg: presenceSweepTick.rowType },
  ctx => {
    runPresenceSweep(ctx, expiredRows(ctx));
  }
);
