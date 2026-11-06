import {
  schema,
  table,
  t,
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
  type ArrayBuilder,
  type Infer,
  type ProcedureExport,
  type ReducerExport,
  type ViewExport,
} from 'spacetimedb/server';
import { errors } from '../errors.js';
import {
  removePresence,
  runPresenceSweep,
  updatePresenceConfig,
  upsertPresence,
} from '../presence.js';
import { presenceConfigRow, presenceEntryRow } from '../tables.js';

const presenceEntryOptions = { name: 'presence_entry', public: false };
export const presenceEntry: ReturnType<
  typeof table<typeof presenceEntryRow, typeof presenceEntryOptions>
> = table(presenceEntryOptions, presenceEntryRow);

const presenceConfigOptions = { name: 'presence_config', public: false };
const presenceConfig: ReturnType<
  typeof table<typeof presenceConfigRow, typeof presenceConfigOptions>
> = table(presenceConfigOptions, presenceConfigRow);

const presenceAdminIdentityOptions = {
  name: 'presence_admin_identity',
  public: false,
};
const presenceAdminIdentityColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
const presenceAdminIdentity: ReturnType<
  typeof table<
    typeof presenceAdminIdentityColumns,
    typeof presenceAdminIdentityOptions
  >
> = table(presenceAdminIdentityOptions, presenceAdminIdentityColumns);

const presenceSweepTickOptions = { name: 'presence_sweep_tick' };
const presenceSweepTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
const presenceSweepTick: ReturnType<
  typeof table<typeof presenceSweepTickColumns, typeof presenceSweepTickOptions>
> = table(presenceSweepTickOptions, presenceSweepTickColumns);

const tables: {
  presenceEntry: typeof presenceEntry;
  presenceConfig: typeof presenceConfig;
  presenceAdminIdentity: typeof presenceAdminIdentity;
  presenceSweepTick: typeof presenceSweepTick;
} = {
  presenceEntry,
  presenceConfig,
  presenceAdminIdentity,
  presenceSweepTick,
};
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
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

const heartbeatArgs = {
  status: t.option(t.string()),
  activity: t.option(t.string()),
  payloadJson: t.option(t.string()),
  ttlSeconds: t.option(t.u32()),
};
/** Records the caller's global presence. The subject is the caller's identity. */
export const heartbeat: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof heartbeatArgs,
  typeof heartbeatResult
> = spacetimedb.procedure(heartbeatArgs, heartbeatResult, (ctx, args) =>
  ctx.withTx(tx => {
    const row = upsertPresence(tx, {
      scope: GLOBAL_SCOPE,
      subject: ctx.sender.toHexString(),
      status: args.status,
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

export const clearPresence: ReducerExport<typeof spacetimedb.schemaType, {}> =
  spacetimedb.reducer({}, ctx => {
    removePresence(ctx, GLOBAL_SCOPE, ctx.sender.toHexString());
  });

const runSweepResult = t.u32();
export const runSweep: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof runSweepResult
> = spacetimedb.procedure({}, runSweepResult, ctx =>
  ctx.withTx(tx => {
    requireAdmin(tx);
    return runPresenceSweep(tx, expiredRows(tx));
  })
);

const addPresenceAdminArgs = { identity: t.identity() };
export const addPresenceAdmin: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof addPresenceAdminArgs
> = spacetimedb.reducer(addPresenceAdminArgs, (ctx, args) => {
  requireAdmin(ctx);
  if (ctx.db.presenceAdminIdentity.identity.find(args.identity) == null) {
    ctx.db.presenceAdminIdentity.insert({
      identity: args.identity,
      addedAt: ctx.timestamp,
    });
  }
});

const removePresenceAdminArgs = { identity: t.identity() };
export const removePresenceAdmin: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof removePresenceAdminArgs
> = spacetimedb.reducer(removePresenceAdminArgs, (ctx, args) => {
  requireAdmin(ctx);
  const row = ctx.db.presenceAdminIdentity.identity.find(args.identity);
  if (!row) return;
  if (ctx.db.presenceAdminIdentity.count() <= 1n) {
    throw new SenderError(errors.cannotRemoveLastAdmin);
  }
  ctx.db.presenceAdminIdentity.delete(row);
});

const updateConfigArgs = { defaultTtlSeconds: t.u32(), sweepBatch: t.u32() };
export const updateConfig: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof updateConfigArgs
> = spacetimedb.reducer(updateConfigArgs, (ctx, args) => {
  requireAdmin(ctx);
  updatePresenceConfig(ctx, args);
});

const presenceOnlineResult: ArrayBuilder<typeof presenceEntry.rowType> =
  t.array(presenceEntry.rowType);
/** Up to 1,000 global presence entries, including their activity and payload. */
export const presenceOnline: ViewExport<
  (
    ctx: ViewCtx<typeof spacetimedb.schemaType>
  ) => Infer<typeof presenceOnlineResult>
> = spacetimedb.view(
  { name: 'presence_online', public: true },
  presenceOnlineResult,
  ctx => takeRows(ctx.db.presenceEntry.scope.filter(GLOBAL_SCOPE))
);

const presenceEntriesAdminResult: ArrayBuilder<typeof presenceEntry.rowType> =
  t.array(presenceEntry.rowType);
export const presenceEntriesAdmin: ViewExport<
  (
    ctx: ViewCtx<typeof spacetimedb.schemaType>
  ) => Infer<typeof presenceEntriesAdminResult>
> = spacetimedb.view(
  { name: 'presence_entries_admin', public: true },
  presenceEntriesAdminResult,
  ctx => (isAdmin(ctx) ? takeRows(ctx.db.presenceEntry.iter()) : [])
);

const presenceSweepArgs: { arg: typeof presenceSweepTick.rowType } = {
  arg: presenceSweepTick.rowType,
};
export const presenceSweep: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof presenceSweepArgs
> = spacetimedb.reducer(
  { onSchedule: presenceSweepTick },
  presenceSweepArgs,
  ctx => {
    runPresenceSweep(ctx, expiredRows(ctx));
  }
);
