import { Timestamp, type Identity } from 'spacetimedb';
import { installPresence } from './install';
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
import {
  DEFAULT_PRESENCE_SWEEP_BATCH,
  DEFAULT_PRESENCE_STATUS,
  MAX_PRESENCE_SWEEP_BATCH,
  removePresence,
  runPresenceSweep,
  sweepPresence,
  updatePresenceConfig,
  upsertPresence,
} from '../index';
import {
  presenceConfigRow,
  presenceEntryRow,
  presenceSweepTickRow,
} from '../tables';

const presenceEntry = table(
  { name: 'presence_entry', public: true },
  presenceEntryRow
);

const presenceConfig = table(
  { name: 'presence_config', public: true },
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
  presenceSweepTickRow
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

const heartbeatResult = t.object('PresenceHeartbeatResult', {
  scope: t.string(),
  subject: t.string(),
  status: t.string(),
  expiresAt: t.timestamp(),
});

const DEFAULT_SCOPE = 'presence.global';

function takeRows<T>(rows: Iterable<T>, limit = 1000): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

function identityHex(identity: Identity): string {
  return identity.toHexString();
}

// Subject is always the sender's Identity, never a caller-supplied value.
function buildSubject(ctx: { sender: Identity }): string {
  return identityHex(ctx.sender);
}

function sanitizeScope(scope: string | undefined): string {
  const out = (scope ?? DEFAULT_SCOPE).trim();
  if (out.length === 0) throw new SenderError('presence.invalid_scope');
  return out;
}

function isAdmin(ctx: ViewCtx<Schema>): boolean {
  return ctx.db.presenceAdminIdentity.identity.find(ctx.sender) != null;
}

function requireAdmin(ctx: Tx): void {
  if (ctx.db.presenceAdminIdentity.identity.find(ctx.sender) == null) {
    throw new SenderError('presence.not_authorized');
  }
}

function toU32(name: string, value: number, max = 0xffff_ffff): number {
  if (!Number.isInteger(value) || value <= 0 || value > max) {
    throw new SenderError(`presence.invalid_${name}`);
  }
  return value;
}

// Fresh publishes seed the publishing owner as admin, install config, and start the expiry sweeper.
export const init = spacetimedb.init(ctx => {
  installPresence(ctx);
});

export const heartbeat = spacetimedb.procedure(
  {
    scope: t.option(t.string()),
    status: t.option(t.string()),
    activity: t.option(t.string()),
    payloadJson: t.option(t.string()),
    ttlSeconds: t.option(t.u32()),
  },
  heartbeatResult,
  (ctx, args) => {
    const scope = sanitizeScope(args.scope);
    const subject = buildSubject(ctx);
    const ttlSeconds =
      args.ttlSeconds === undefined
        ? undefined
        : toU32('ttl_seconds', Number(args.ttlSeconds));

    let out: {
      scope: string;
      subject: string;
      status: string;
      expiresAt: Timestamp;
    } | null = null;
    ctx.withTx(tx => {
      const row = upsertPresence(tx, {
        scope,
        subject,
        status: args.status ?? DEFAULT_PRESENCE_STATUS,
        activity: args.activity,
        payloadJson: args.payloadJson,
        ttlSeconds,
      });
      out = {
        scope: row.scope,
        subject: row.subject,
        status: row.status,
        expiresAt: row.expiresAt,
      };
    });
    if (!out) throw new SenderError('presence.heartbeat_tx_failed');
    return out;
  }
);

export const clear_presence = spacetimedb.reducer(
  { scope: t.option(t.string()) },
  (ctx, args) => {
    const scope = sanitizeScope(args.scope);
    const subject = buildSubject(ctx);
    const tx: Tx = ctx;
    removePresence(tx, scope, subject);
  }
);

export const run_sweep = spacetimedb.procedure(
  { maxRows: t.option(t.u32()) },
  t.u32(),
  (ctx, args) => {
    const maxRows =
      args.maxRows === undefined
        ? undefined
        : toU32('sweep_batch', Number(args.maxRows), MAX_PRESENCE_SWEEP_BATCH);
    let deleted = 0;
    ctx.withTx(tx => {
      requireAdmin(tx);
      deleted = sweepPresence(
        tx,
        tx.db.presenceEntry.expiresAt.filter(
          new Range(undefined, { tag: 'included', value: tx.timestamp })
        ),
        maxRows ?? DEFAULT_PRESENCE_SWEEP_BATCH
      );
    });
    return deleted;
  }
);

export const add_presence_admin = spacetimedb.reducer(
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

export const update_config = spacetimedb.reducer(
  { defaultTtlSeconds: t.u32(), sweepBatch: t.u32() },
  (ctx, args) => {
    requireAdmin(ctx);
    updatePresenceConfig(ctx, {
      defaultTtlSeconds: toU32(
        'default_ttl_seconds',
        Number(args.defaultTtlSeconds)
      ),
      sweepBatch: toU32(
        'sweep_batch',
        Number(args.sweepBatch),
        MAX_PRESENCE_SWEEP_BATCH
      ),
    });
  }
);

export const presenceEntriesAdmin = spacetimedb.view(
  { name: 'presence_entries_admin', public: true },
  t.array(presenceEntry.rowType),
  ctx => (isAdmin(ctx) ? takeRows(ctx.db.presenceEntry.iter()) : [])
);

export const presence_sweep = spacetimedb.reducer(
  { onSchedule: presenceSweepTick },
  { arg: presenceSweepTick.rowType },
  (ctx, _args) => {
    runPresenceSweep(
      ctx,
      ctx.db.presenceEntry.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      )
    );
  }
);
