import {
  schema,
  t,
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
  type ReducerExport,
} from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import { errors } from '../errors.js';
import { BUILT_IN_PROVIDERS } from '../providers.js';
import {
  deleteStaleThreadLocks,
  staleLockCutoffMicros,
} from '../stale-locks.js';
import {
  apiKey,
  agentConfig,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  threadLockSweeperTick,
  messageEmbedding,
} from './model.js';

const ONE_SECOND_MICROS = 1_000_000n;
export const DEFAULT_STALE_LOCK_THRESHOLD_SECS = 15 * 60;

const tables: {
  apiKey: typeof apiKey;
  agentConfig: typeof agentConfig;
  agentAdminIdentity: typeof agentAdminIdentity;
  agentOverride: typeof agentOverride;
  thread: typeof thread;
  message: typeof message;
  threadLock: typeof threadLock;
  threadLockSweeperTick: typeof threadLockSweeperTick;
  messageEmbedding: typeof messageEmbedding;
} = {
  apiKey,
  agentConfig,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  threadLockSweeperTick,
  messageEmbedding,
};
const spacetimedb: ReturnType<typeof schema<typeof tables>> = schema(tables);
export default spacetimedb;

export type AgentsTx = ReducerCtx<InferSchema<typeof spacetimedb>>;

export function isAgentAdmin(tx: AgentsTx): boolean {
  return tx.db.agentAdminIdentity.identity.find(tx.sender) != null;
}

function requireAdmin(tx: AgentsTx): void {
  if (!isAgentAdmin(tx)) throw new SenderError(errors.notAuthorized);
}

function requireKnownProvider(provider: string): void {
  if (!Object.hasOwn(BUILT_IN_PROVIDERS, provider)) {
    throw new SenderError(`${errors.unknownProvider}:${provider}`);
  }
}

const setAgentConfigArgs = { staleLockThresholdSecs: t.option(t.u32()) };
export const setAgentConfig: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof setAgentConfigArgs
> = spacetimedb.reducer(setAgentConfigArgs, (ctx, args) => {
  requireAdmin(ctx);
  const staleLockThresholdSecs =
    args.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS;
  if (staleLockThresholdSecs === 0) {
    throw new SenderError(errors.invalidStaleLockThreshold);
  }
  const row = {
    singleton: true,
    staleLockThresholdSecs,
    updatedAt: ctx.timestamp,
  };
  if (ctx.db.agentConfig.singleton.find(true)) {
    ctx.db.agentConfig.singleton.update(row);
  } else {
    ctx.db.agentConfig.insert(row);
  }
});

const setApiKeyArgs = { provider: t.string(), key: t.string() };
export const setApiKey: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof setApiKeyArgs
> = spacetimedb.reducer(setApiKeyArgs, (ctx, args) => {
  requireAdmin(ctx);
  requireKnownProvider(args.provider);
  if (args.key.length === 0) throw new SenderError(errors.invalidApiKey);
  const row = {
    provider: args.provider,
    key: args.key,
    updatedAt: ctx.timestamp,
  };
  if (ctx.db.apiKey.provider.find(args.provider)) {
    ctx.db.apiKey.provider.update(row);
  } else {
    ctx.db.apiKey.insert(row);
  }
});

const clearApiKeyArgs = { provider: t.string() };
export const clearApiKey: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof clearApiKeyArgs
> = spacetimedb.reducer(clearApiKeyArgs, (ctx, { provider }) => {
  requireAdmin(ctx);
  ctx.db.apiKey.provider.delete(provider);
});

const setAgentOverrideArgs = {
  agentName: t.string(),
  provider: t.option(t.string()),
  model: t.option(t.string()),
  systemPrompt: t.option(t.string()),
  maxTurns: t.option(t.u32()),
  maxHistoryMessages: t.option(t.u32()),
  maxTokens: t.option(t.u32()),
  retries: t.option(t.u32()),
};
// Overrides for names that no client agent defines are stored but unused.
export const setAgentOverride: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof setAgentOverrideArgs
> = spacetimedb.reducer(setAgentOverrideArgs, (ctx, args) => {
  requireAdmin(ctx);
  if (args.provider !== undefined) requireKnownProvider(args.provider);
  if (args.maxTurns === 0) throw new SenderError(errors.invalidMaxTurns);
  if (args.maxHistoryMessages === 0) {
    throw new SenderError(errors.invalidMaxHistory);
  }
  const row = {
    agentName: args.agentName,
    provider: args.provider,
    model: args.model,
    systemPrompt: args.systemPrompt,
    maxTurns: args.maxTurns,
    maxHistoryMessages: args.maxHistoryMessages,
    maxTokens: args.maxTokens,
    retries: args.retries,
    updatedAt: ctx.timestamp,
  };
  if (ctx.db.agentOverride.agentName.find(args.agentName)) {
    ctx.db.agentOverride.agentName.update(row);
  } else {
    ctx.db.agentOverride.insert(row);
  }
});

const clearAgentOverrideArgs = { agentName: t.string() };
export const clearAgentOverride: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof clearAgentOverrideArgs
> = spacetimedb.reducer(clearAgentOverrideArgs, (ctx, { agentName }) => {
  requireAdmin(ctx);
  ctx.db.agentOverride.agentName.delete(agentName);
});

const addAgentAdminIdentityArgs = { identity: t.identity() };
export const addAgentAdminIdentity: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof addAgentAdminIdentityArgs
> = spacetimedb.reducer(addAgentAdminIdentityArgs, (ctx, { identity }) => {
  requireAdmin(ctx);
  if (ctx.db.agentAdminIdentity.identity.find(identity) == null) {
    ctx.db.agentAdminIdentity.insert({
      identity,
      addedAt: ctx.timestamp,
    });
  }
});

const removeAgentAdminIdentityArgs = { identity: t.identity() };
export const removeAgentAdminIdentity: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof removeAgentAdminIdentityArgs
> = spacetimedb.reducer(removeAgentAdminIdentityArgs, (ctx, { identity }) => {
  requireAdmin(ctx);
  if (ctx.db.agentAdminIdentity.identity.find(identity) == null) return;
  if (ctx.db.agentAdminIdentity.count() <= 1n) {
    throw new SenderError(errors.cannotRemoveLastAdmin);
  }
  ctx.db.agentAdminIdentity.identity.delete(identity);
});

const clearThreadLockArgs = { threadId: t.u64() };
// Bypasses ownership to clear a wedged lock.
export const clearThreadLock: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof clearThreadLockArgs
> = spacetimedb.reducer(clearThreadLockArgs, (ctx, { threadId }) => {
  requireAdmin(ctx);
  ctx.db.threadLock.threadId.delete(threadId);
});

const threadLockSweepArgs: { arg: typeof threadLockSweeperTick.rowType } = {
  arg: threadLockSweeperTick.rowType,
};
export const threadLockSweep: ReducerExport<
  typeof spacetimedb.schemaType,
  typeof threadLockSweepArgs
> = spacetimedb.reducer(
  { onSchedule: threadLockSweeperTick },
  threadLockSweepArgs,
  ctx => {
    const thresholdSecs =
      ctx.db.agentConfig.singleton.find(true)?.staleLockThresholdSecs ??
      DEFAULT_STALE_LOCK_THRESHOLD_SECS;
    const cutoffMicros = staleLockCutoffMicros(
      ctx.timestamp.microsSinceUnixEpoch,
      BigInt(thresholdSecs) * ONE_SECOND_MICROS
    );
    deleteStaleThreadLocks(
      ctx.db.threadLock.lockedAt.filter(
        new Range(undefined, {
          tag: 'excluded',
          value: new Timestamp(cutoffMicros),
        })
      ),
      cutoffMicros,
      lock => ctx.db.threadLock.delete(lock)
    );
  }
);
