import {
  schema,
  t,
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import { errors } from '../errors';
import { BUILT_IN_PROVIDERS } from '../providers';
import { deleteStaleThreadLocks, staleLockCutoffMicros } from '../stale-locks';
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
} from './model';

const ONE_SECOND_MICROS = 1_000_000n;
export const DEFAULT_STALE_LOCK_THRESHOLD_SECS = 15 * 60;

const spacetimedb = schema({
  apiKey,
  agentConfig,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  threadLockSweeperTick,
  messageEmbedding,
});
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

export const setAgentConfig = spacetimedb.reducer(
  { staleLockThresholdSecs: t.option(t.u32()) },
  (ctx, args) => {
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
  }
);

export const setApiKey = spacetimedb.reducer(
  { provider: t.string(), key: t.string() },
  (ctx, args) => {
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
  }
);

export const clearApiKey = spacetimedb.reducer(
  { provider: t.string() },
  (ctx, { provider }) => {
    requireAdmin(ctx);
    ctx.db.apiKey.provider.delete(provider);
  }
);

// Overrides for names that no client agent defines are stored but unused.
export const setAgentOverride = spacetimedb.reducer(
  {
    agentName: t.string(),
    provider: t.option(t.string()),
    model: t.option(t.string()),
    systemPrompt: t.option(t.string()),
    maxTurns: t.option(t.u32()),
    maxHistoryMessages: t.option(t.u32()),
    maxTokens: t.option(t.u32()),
    retries: t.option(t.u32()),
  },
  (ctx, args) => {
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
  }
);

export const clearAgentOverride = spacetimedb.reducer(
  { agentName: t.string() },
  (ctx, { agentName }) => {
    requireAdmin(ctx);
    ctx.db.agentOverride.agentName.delete(agentName);
  }
);

export const addAgentAdminIdentity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    requireAdmin(ctx);
    if (ctx.db.agentAdminIdentity.identity.find(identity) == null) {
      ctx.db.agentAdminIdentity.insert({
        identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const removeAgentAdminIdentity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    requireAdmin(ctx);
    if (ctx.db.agentAdminIdentity.identity.find(identity) == null) return;
    if (ctx.db.agentAdminIdentity.count() <= 1n) {
      throw new SenderError(errors.cannotRemoveLastAdmin);
    }
    ctx.db.agentAdminIdentity.identity.delete(identity);
  }
);

// Bypasses ownership to clear a wedged lock.
export const clearThreadLock = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    requireAdmin(ctx);
    ctx.db.threadLock.threadId.delete(threadId);
  }
);

export const threadLockSweep = spacetimedb.reducer(
  { onSchedule: threadLockSweeperTick },
  { arg: threadLockSweeperTick.rowType },
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
