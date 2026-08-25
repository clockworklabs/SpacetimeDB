import {
  schema,
  table,
  t,
  Range,
  SenderError,
  type TransactionCtx,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import { Timestamp, type Identity } from 'spacetimedb';
import {
  deleteStaleThreadLocks,
  staleLockCutoffMicros,
} from '@spacetimedb/agents/stale-locks';
import { installAgents } from './install';
import {
  agentTool,
  defineAgent,
  makeAgentRegistry,
} from '@spacetimedb/agents/kit';
import {
  callChat,
  type ChatMessage,
  type Provider,
  type HttpLike,
} from '@spacetimedb/agents/openrouter';
import { BUILT_IN_PROVIDERS } from '@spacetimedb/agents/providers';
import {
  BUILT_IN_EMBEDDING_PROVIDERS,
  cosineSimilarity,
  topKByScore,
} from '@spacetimedb/agents/embeddings';
import {
  runAgentLoop,
  USER_CONTENT_MAX,
  type LoopConfig,
  type LoopMessage,
  type LoopTx,
} from './loop';
import {
  augmentSystemWithSummary,
  buildSummarizerUserContent,
  pickSummarizationCandidates,
} from './summarize';

const ONE_SECOND_MICROS = 1_000_000n;
const DEFAULT_STALE_LOCK_THRESHOLD_SECS = 15 * 60;

function throwSenderError(msg: string): never {
  throw new SenderError(msg);
}

const echo = agentTool(
  'echoes the given message back to the caller',
  t.object('EchoArgs', { message: t.string() }),
  (_ctx, args) => `echo: ${args.message}`
);

const getTime = agentTool(
  'returns the current server time as an ISO-8601 string',
  t.unit(),
  ctx => {
    const tx = ctx as { timestamp: { microsSinceUnixEpoch: bigint } };
    const micros = tx.timestamp.microsSinceUnixEpoch;
    return new Date(Number(micros / 1000n)).toISOString();
  }
);

const chatAgent = defineAgent({
  defaultModel: 'anthropic/claude-haiku-4.5',
  defaultSystemPrompt:
    'You are a helpful assistant. Use tools when they make the answer better.',
  defaultMaxTurns: 10,
  defaultMaxHistoryMessages: 50,
  defaultRetries: 2,
  summarizerAgentName: 'summarizer',
  embeddingsProvider: 'openai',
  embeddingsModel: 'text-embedding-3-small',
  ragTopK: 4,
  tools: {
    get_time: getTime,
    echo,
  },
});

const summarizerAgent = defineAgent({
  defaultModel: 'anthropic/claude-haiku-4.5',
  defaultSystemPrompt:
    'You produce concise running summaries of chat conversations. ' +
    'Capture facts, decisions, names, numbers, and ongoing tasks the ' +
    'main assistant must remember. Skip pleasantries. If the user ' +
    'provides an existing summary, EXTEND it with the new content. ' +
    'Do not restart from scratch and do not duplicate prior facts. ' +
    'Reply with the updated summary as plain prose, no preamble.',
  defaultMaxTurns: 1,
  defaultMaxHistoryMessages: 100,
  defaultMaxTokens: 600,
  defaultRetries: 2,
  tools: {},
});

const agents = {
  chat: chatAgent,
  summarizer: summarizerAgent,
};

import {
  apiKey,
  agentSecret,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  messageEmbedding,
} from './model';

const threadLockSweeperTick = table(
  { name: 'thread_lock_sweeper_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({
  apiKey,
  agentSecret,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  threadLockSweeperTick,
  messageEmbedding,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type WriteCtx = TransactionCtx<Schema>;

const registry = makeAgentRegistry<WriteCtx, typeof agents>(agents);

export const myThreads = spacetimedb.view(
  { name: 'my_threads', public: true },
  t.array(thread.rowType),
  ctx => [...ctx.db.thread.owner.filter(ctx.sender)]
);

export const myMessages = spacetimedb.view(
  { name: 'my_messages', public: true },
  t.array(message.rowType),
  ctx => [...ctx.db.message.owner.filter(ctx.sender)]
);

export const myThreadLocks = spacetimedb.view(
  { name: 'my_thread_locks', public: true },
  t.array(threadLock.rowType),
  ctx => [...ctx.db.threadLock.owner.filter(ctx.sender)]
);

export const myMessageEmbeddings = spacetimedb.view(
  { name: 'my_message_embeddings', public: true },
  t.array(messageEmbedding.rowType),
  ctx => [...ctx.db.messageEmbedding.owner.filter(ctx.sender)]
);

function requireAdmin(tx: WriteCtx): void {
  if (tx.db.agentAdminIdentity.identity.find(tx.sender) == null) {
    throwSenderError('agent.not_authorized');
  }
}

type CallerCtx = ProcedureCtx<Schema> | ReducerCtx<Schema>;

function callerIdentity(ctx: CallerCtx): Identity {
  return ctx.sender;
}

function requireOwnedThread(tx: WriteCtx, threadId: bigint, owner: Identity) {
  const row = tx.db.thread.id.find(threadId);
  if (!row) throwSenderError(`agent.thread_not_found:${threadId}`);
  if (!row.owner.isEqual(owner)) {
    throwSenderError(`agent.not_thread_owner:${threadId}`);
  }
  return row;
}

export const init = spacetimedb.init(ctx => {
  installAgents(ctx);
});

export const set_agent_secret = spacetimedb.reducer(
  { staleLockThresholdSecs: t.option(t.u32()) },
  (ctx, args) => {
    const staleLockThresholdSecs =
      args.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS;
    if (staleLockThresholdSecs === 0) {
      throwSenderError('agent.invalid_stale_lock_threshold:must be > 0');
    }

    const tx = ctx;
    requireAdmin(tx);

    const existing = tx.db.agentSecret.singleton.find(true);
    const row = {
      singleton: true,
      staleLockThresholdSecs,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.agentSecret.singleton.update(row);
    } else {
      tx.db.agentSecret.insert(row);
    }
  }
);

export const set_api_key = spacetimedb.reducer(
  { provider: t.string(), key: t.string() },
  (ctx, args) => {
    if (args.provider.length === 0)
      throwSenderError('agent.invalid_provider:empty');
    if (args.key.length === 0) throwSenderError('agent.invalid_api_key:empty');
    if (!Object.hasOwn(BUILT_IN_PROVIDERS, args.provider)) {
      throwSenderError(`agent.unknown_provider:${args.provider}`);
    }
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.apiKey.provider.find(args.provider);
    const row = {
      provider: args.provider,
      key: args.key,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.apiKey.provider.update(row);
    } else {
      tx.db.apiKey.insert(row);
    }
  }
);

export const clear_api_key = spacetimedb.reducer(
  { provider: t.string() },
  (ctx, { provider }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.apiKey.provider.find(provider);
    if (existing) tx.db.apiKey.delete(existing);
  }
);

export const set_agent_override = spacetimedb.reducer(
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
    if (!registry.has(args.agentName)) {
      throwSenderError(`agent.unknown:${args.agentName}`);
    }
    if (
      args.provider !== undefined &&
      !Object.hasOwn(BUILT_IN_PROVIDERS, args.provider)
    ) {
      throwSenderError(`agent.unknown_provider:${args.provider}`);
    }
    if (args.maxTurns !== undefined && args.maxTurns === 0) {
      throwSenderError('agent.invalid_max_turns:must be > 0');
    }
    if (
      args.maxHistoryMessages !== undefined &&
      args.maxHistoryMessages === 0
    ) {
      throwSenderError('agent.invalid_max_history:must be > 0');
    }

    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentOverride.agentName.find(args.agentName);
    const row = {
      agentName: args.agentName,
      provider: args.provider,
      model: args.model,
      systemPrompt: args.systemPrompt,
      maxTurns: args.maxTurns,
      maxHistoryMessages: args.maxHistoryMessages,
      maxTokens: args.maxTokens,
      retries: args.retries,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.agentOverride.agentName.update(row);
    } else {
      tx.db.agentOverride.insert(row);
    }
  }
);

export const clear_agent_override = spacetimedb.reducer(
  { agentName: t.string() },
  (ctx, { agentName }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentOverride.agentName.find(agentName);
    if (existing) tx.db.agentOverride.delete(existing);
  }
);

export const add_agent_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    const tx = ctx;
    requireAdmin(tx);
    if (tx.db.agentAdminIdentity.identity.find(identity) == null) {
      tx.db.agentAdminIdentity.insert({
        identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const remove_agent_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentAdminIdentity.identity.find(identity);
    if (!existing) return;
    if (tx.db.agentAdminIdentity.count() <= 1n) {
      throwSenderError('agent.cannot_remove_last_admin');
    }
    tx.db.agentAdminIdentity.delete(existing);
  }
);

export const get_agent_config_status = spacetimedb.procedure(
  {},
  t.object('AgentConfigStatus', {
    isConfigured: t.bool(),
    staleLockThresholdSecs: t.u32(),
    agents: t.array(
      t.object('AgentInfo', {
        name: t.string(),
        defaultProvider: t.string(),
        defaultModel: t.string(),
      })
    ),
    configuredProviders: t.array(t.string()),
  }),
  ctx =>
    ctx.withTx(tx => {
      const secret = tx.db.agentSecret.singleton.find(true);
      const configuredProviders = [...tx.db.apiKey.iter()]
        .map(r => r.provider)
        .sort();
      const agentInfos = registry.names().map(name => {
        const def = registry.agentDef(name)!;
        return {
          name,
          defaultProvider: def.defaultProvider,
          defaultModel: def.defaultModel,
        };
      });
      return {
        isConfigured: secret != null,
        staleLockThresholdSecs:
          secret?.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS,
        agents: agentInfos,
        configuredProviders,
      };
    })
);

export const start_thread = spacetimedb.procedure(
  {
    agentName: t.string(),
    title: t.option(t.string()),
    systemPromptOverride: t.option(t.string()),
    metadata: t.option(t.string()),
  },
  t.u64(),
  (ctx, args) => {
    const owner = callerIdentity(ctx);
    if (!registry.has(args.agentName)) {
      throwSenderError(`agent.unknown:${args.agentName}`);
    }
    return ctx.withTx(tx => {
      const inserted = tx.db.thread.insert({
        id: 0n,
        owner,
        agentName: args.agentName,
        title: args.title,
        systemPromptOverride: args.systemPromptOverride,
        modelOverride: undefined,
        metadata: args.metadata,
        summary: undefined,
        summarizedThroughId: undefined,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });
      return inserted.id;
    });
  }
);

export const update_thread = spacetimedb.reducer(
  {
    threadId: t.u64(),
    title: t.option(t.string()),
    systemPromptOverride: t.option(t.string()),
    modelOverride: t.option(t.string()),
    metadata: t.option(t.string()),
    clearTitle: t.bool(),
    clearSystemPromptOverride: t.bool(),
    clearModelOverride: t.bool(),
    clearMetadata: t.bool(),
  },
  (ctx, args) => {
    const owner = callerIdentity(ctx);
    const tx = ctx;
    const row = requireOwnedThread(tx, args.threadId, owner);
    tx.db.thread.id.update({
      ...row,
      title: args.clearTitle ? undefined : (args.title ?? row.title),
      systemPromptOverride: args.clearSystemPromptOverride
        ? undefined
        : (args.systemPromptOverride ?? row.systemPromptOverride),
      modelOverride: args.clearModelOverride
        ? undefined
        : (args.modelOverride ?? row.modelOverride),
      metadata: args.clearMetadata
        ? undefined
        : (args.metadata ?? row.metadata),
      updatedAt: tx.timestamp,
    });
  }
);

export const delete_thread = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const owner = callerIdentity(ctx);
    const tx = ctx;
    const row = requireOwnedThread(tx, threadId, owner);
    if (tx.db.threadLock.threadId.find(threadId) != null) {
      throwSenderError(`agent.thread_busy:${threadId}`);
    }
    for (const e of [...tx.db.messageEmbedding.threadId.filter(threadId)]) {
      tx.db.messageEmbedding.delete(e);
    }
    for (const m of [...tx.db.message.threadId.filter(threadId)]) {
      tx.db.message.delete(m);
    }
    tx.db.thread.delete(row);
  }
);

// Admin-gated and bypasses ownership, to clear a wedged lock.
export const clear_thread_lock = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const tx = ctx;
    requireAdmin(tx);
    const lock = tx.db.threadLock.threadId.find(threadId);
    if (lock) tx.db.threadLock.delete(lock);
  }
);

export const request_cancel = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const owner = callerIdentity(ctx);
    const tx = ctx;
    requireOwnedThread(tx, threadId, owner);
    const lock = tx.db.threadLock.threadId.find(threadId);
    if (!lock) throwSenderError(`agent.thread_not_running:${threadId}`);
    if (lock.cancelRequested) return;
    tx.db.threadLock.threadId.update({ ...lock, cancelRequested: true });
  }
);

function resolveProvider(name: string): Provider {
  const p = BUILT_IN_PROVIDERS[name];
  if (!p) throwSenderError(`agent.unknown_provider:${name}`);
  return p;
}

function loadLoopConfigOrThrow(
  tx: WriteCtx,
  threadId: bigint,
  owner: Identity
): { cfg: LoopConfig; agentName: string; owner: Identity } {
  const threadRow = requireOwnedThread(tx, threadId, owner);

  const def = registry.agentDef(threadRow.agentName);
  if (!def) {
    throwSenderError(`agent.unknown:${threadRow.agentName}`);
  }

  if (tx.db.threadLock.threadId.find(threadId) != null) {
    throwSenderError(`agent.thread_busy:${threadId}`);
  }
  if (tx.db.agentSecret.singleton.find(true) == null) {
    throwSenderError('agent.not_configured');
  }

  const override = tx.db.agentOverride.agentName.find(threadRow.agentName);
  const providerName = override?.provider ?? def.defaultProvider;
  const provider = resolveProvider(providerName);

  const keyRow = tx.db.apiKey.provider.find(providerName);
  if (!keyRow) throwSenderError(`agent.no_api_key:${providerName}`);

  return {
    cfg: {
      provider,
      apiKey: keyRow.key,
      model: threadRow.modelOverride ?? override?.model ?? def.defaultModel,
      systemPrompt:
        threadRow.systemPromptOverride ??
        override?.systemPrompt ??
        def.defaultSystemPrompt,
      maxTurns: override?.maxTurns ?? def.defaultMaxTurns,
      maxHistoryMessages:
        override?.maxHistoryMessages ?? def.defaultMaxHistoryMessages,
      maxTokens: override?.maxTokens ?? def.defaultMaxTokens,
      retries: override?.retries ?? def.defaultRetries,
      responseFormat: def.defaultResponseFormat,
    },
    agentName: threadRow.agentName,
    owner: threadRow.owner,
  };
}

function augmentSystemWithRag(
  base: string | undefined,
  snippets: string[]
): string | undefined {
  if (snippets.length === 0) return base;
  const b = base ?? '';
  return `${b}\n\n## Relevant earlier messages\n${snippets.join('\n---\n')}`.trim();
}

type ProcLikeCtx = {
  http: HttpLike;
  withTx: <R>(fn: (tx: WriteCtx) => R) => R;
};

function threadMessagesAscending(tx: WriteCtx, threadId: bigint) {
  const rows = [...tx.db.message.threadId.filter(threadId)];
  rows.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  return rows;
}

function toLoopMessage(r: {
  id: bigint;
  threadId: bigint;
  role: string;
  content: string;
  toolCallsJson: string | undefined;
  toolCallId: string | undefined;
  isError: boolean;
  promptTokens: number | undefined;
  completionTokens: number | undefined;
}): LoopMessage {
  return {
    id: r.id,
    threadId: r.threadId,
    role: r.role,
    content: r.content,
    toolCallsJson: r.toolCallsJson,
    toolCallId: r.toolCallId,
    isError: r.isError,
    promptTokens: r.promptTokens,
    completionTokens: r.completionTokens,
  };
}

function maybeEmbedMessage(
  ctx: ProcLikeCtx,
  threadId: bigint,
  messageId: bigint
): void {
  const job = ctx.withTx(tx => {
    if (tx.db.messageEmbedding.messageId.find(messageId) != null) return null;
    const msg = tx.db.message.id.find(messageId);
    if (!msg) return null;
    const threadRow = tx.db.thread.id.find(threadId);
    if (!threadRow) return null;
    const def = registry.agentDef(threadRow.agentName);
    if (!def?.embeddingsProvider || !def.embeddingsModel) return null;
    const provider = BUILT_IN_EMBEDDING_PROVIDERS[def.embeddingsProvider];
    if (!provider) return null;
    const keyRow = tx.db.apiKey.provider.find(def.embeddingsProvider);
    if (!keyRow) return null;
    return {
      provider,
      apiKey: keyRow.key,
      model: def.embeddingsModel,
      content: msg.content,
      owner: msg.owner,
    };
  });
  if (!job) return;

  const result = job.provider.embed(ctx.http, job.apiKey, job.model, [
    job.content,
  ]);
  if (!result.ok || result.vectors.length === 0) {
    console.warn(
      `embedding failed: ${result.ok ? 'no vectors' : result.error.kind}`
    );
    return;
  }
  ctx.withTx(tx => {
    if (tx.db.messageEmbedding.messageId.find(messageId) != null) return;
    tx.db.messageEmbedding.insert({
      messageId,
      threadId,
      owner: job.owner,
      model: job.model,
      vector: result.vectors[0],
      createdAt: tx.timestamp,
    });
  });
}

function maybeRetrieveRag(ctx: ProcLikeCtx, threadId: bigint): string[] {
  return ctx.withTx(tx => {
    const threadRow = tx.db.thread.id.find(threadId);
    if (!threadRow) return [];
    const def = registry.agentDef(threadRow.agentName);
    if (!def || def.ragTopK <= 0) return [];

    const msgs = threadMessagesAscending(tx, threadId);
    let queryMsg = undefined as (typeof msgs)[number] | undefined;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === 'user') {
        queryMsg = msgs[i];
        break;
      }
    }
    if (!queryMsg) return [];
    const queryEmb = tx.db.messageEmbedding.messageId.find(queryMsg.id);
    if (!queryEmb) return [];

    const override = tx.db.agentOverride.agentName.find(threadRow.agentName);
    const maxHistory =
      override?.maxHistoryMessages ?? def.defaultMaxHistoryMessages;
    const windowStartIdx = Math.max(0, msgs.length - maxHistory);
    const inWindowIds = new Set(msgs.slice(windowStartIdx).map(m => m.id));

    const candidates = [
      ...tx.db.messageEmbedding.threadId.filter(threadId),
    ].filter(
      e => !inWindowIds.has(e.messageId) && e.messageId !== queryMsg!.id
    );
    const top = topKByScore(
      candidates,
      e => cosineSimilarity(queryEmb.vector, e.vector),
      def.ragTopK
    ).filter(x => x.score > 0);

    const out: string[] = [];
    for (const { item } of top) {
      const m = tx.db.message.id.find(item.messageId);
      if (m) out.push(`[${m.role}] ${m.content}`);
    }
    return out;
  });
}

function maybeRunSummarization(ctx: ProcLikeCtx, threadId: bigint): void {
  const decision = ctx.withTx(tx => {
    const threadRow = tx.db.thread.id.find(threadId);
    if (!threadRow) return null;
    const def = registry.agentDef(threadRow.agentName);
    if (!def?.summarizerAgentName) return null;
    const sumDef = registry.agentDef(def.summarizerAgentName);
    if (!sumDef) return null;

    const override = tx.db.agentOverride.agentName.find(threadRow.agentName);
    const maxHistory =
      override?.maxHistoryMessages ?? def.defaultMaxHistoryMessages;

    const rows = threadMessagesAscending(tx, threadId).map(toLoopMessage);

    const candidates = pickSummarizationCandidates(
      rows,
      maxHistory,
      threadRow.summarizedThroughId ?? null
    );
    if (!candidates) return null;

    const sumOverride = tx.db.agentOverride.agentName.find(
      def.summarizerAgentName
    );
    const sumProviderName = sumOverride?.provider ?? sumDef.defaultProvider;
    const sumProvider = BUILT_IN_PROVIDERS[sumProviderName];
    if (!sumProvider) return null;
    const keyRow = tx.db.apiKey.provider.find(sumProviderName);
    if (!keyRow) return null;

    return {
      provider: sumProvider,
      apiKey: keyRow.key,
      sumModel: sumOverride?.model ?? sumDef.defaultModel,
      sumSystemPrompt: sumOverride?.systemPrompt ?? sumDef.defaultSystemPrompt,
      sumMaxTokens: sumOverride?.maxTokens ?? sumDef.defaultMaxTokens,
      sumRetries: sumOverride?.retries ?? sumDef.defaultRetries,
      existingSummary: threadRow.summary ?? null,
      newDropped: candidates.newDropped,
      lastNewId: candidates.lastNewId,
    };
  });
  if (!decision) return;

  const userContent = buildSummarizerUserContent(
    decision.existingSummary,
    decision.newDropped
  );
  const messages: ChatMessage[] = [{ role: 'user', content: userContent }];
  const result = callChat(ctx.http, decision.provider, {
    apiKey: decision.apiKey,
    model: decision.sumModel,
    system: decision.sumSystemPrompt,
    messages,
    maxTokens: decision.sumMaxTokens,
    retries: decision.sumRetries,
  });
  if (!result.ok || !result.response.text) {
    console.warn(
      `summarization failed: ${result.ok ? 'no text in response' : result.error.kind}`
    );
    return;
  }

  ctx.withTx(tx => {
    const threadRow = tx.db.thread.id.find(threadId);
    if (!threadRow) return;
    tx.db.thread.id.update({
      ...threadRow,
      summary: result.response.text!,
      summarizedThroughId: decision.lastNewId,
      updatedAt: tx.timestamp,
    });
  });
}

function adaptTx(tx: WriteCtx, agentName: string, owner: Identity): LoopTx {
  return {
    listMessages(threadId: bigint): LoopMessage[] {
      return threadMessagesAscending(tx, threadId).map(toLoopMessage);
    },
    appendMessage(row) {
      tx.db.message.insert({
        id: 0n,
        threadId: row.threadId,
        owner,
        role: row.role,
        content: row.content,
        toolCallsJson: row.toolCallsJson,
        toolCallId: row.toolCallId,
        isError: row.isError,
        promptTokens: row.promptTokens,
        completionTokens: row.completionTokens,
        createdAt: tx.timestamp,
      });
    },
    bumpThread(threadId: bigint): void {
      const r = tx.db.thread.id.find(threadId);
      if (r) tx.db.thread.id.update({ ...r, updatedAt: tx.timestamp });
    },
    invokeTool(name: string, inputJson: string) {
      return registry.invoke(agentName, tx, name, inputJson);
    },
    isCancelRequested(threadId: bigint): boolean {
      const lock = tx.db.threadLock.threadId.find(threadId);
      return lock != null && lock.cancelRequested;
    },
  };
}

function runLockedLoop(
  ctx: ProcLikeCtx,
  cfg: LoopConfig,
  agentName: string,
  owner: Identity,
  threadId: bigint
): void {
  try {
    maybeRunSummarization(ctx, threadId);
    const ragSnippets = maybeRetrieveRag(ctx, threadId);

    const finalCfg = ctx.withTx(tx => {
      const threadRow = tx.db.thread.id.find(threadId);
      if (!threadRow) return cfg;
      let systemPrompt = cfg.systemPrompt;
      systemPrompt = augmentSystemWithSummary(
        systemPrompt,
        threadRow.summary ?? null
      );
      systemPrompt = augmentSystemWithRag(systemPrompt, ragSnippets);
      return { ...cfg, systemPrompt };
    });

    runAgentLoop({
      http: ctx.http,
      withTx: <R>(fn: (lt: LoopTx) => R): R =>
        ctx.withTx(tx => fn(adaptTx(tx, agentName, owner))),
      llmToolDefs: registry.llmToolDefsFor(agentName),
      cfg: finalCfg,
      threadId,
    });
  } finally {
    ctx.withTx(tx => {
      const lock = tx.db.threadLock.threadId.find(threadId);
      if (lock) tx.db.threadLock.delete(lock);
    });
  }
}

export const send_message = spacetimedb.procedure(
  { threadId: t.u64(), content: t.string() },
  t.unit(),
  (ctx, args) => {
    if (args.content.length === 0) {
      throwSenderError('agent.empty_message');
    }
    const content =
      args.content.length > USER_CONTENT_MAX
        ? args.content.slice(0, USER_CONTENT_MAX) + '...[truncated]'
        : args.content;

    const owner = callerIdentity(ctx);
    const {
      cfg,
      agentName,
      owner: threadOwner,
      userMessageId,
    } = ctx.withTx(tx => {
      const loaded = loadLoopConfigOrThrow(tx, args.threadId, owner);
      tx.db.threadLock.insert({
        threadId: args.threadId,
        owner: loaded.owner,
        lockedAt: tx.timestamp,
        cancelRequested: false,
      });
      const inserted = tx.db.message.insert({
        id: 0n,
        threadId: args.threadId,
        owner: loaded.owner,
        role: 'user',
        content,
        toolCallsJson: undefined,
        toolCallId: undefined,
        isError: false,
        promptTokens: undefined,
        completionTokens: undefined,
        createdAt: tx.timestamp,
      });
      const threadRow = tx.db.thread.id.find(args.threadId);
      if (threadRow)
        tx.db.thread.id.update({ ...threadRow, updatedAt: tx.timestamp });
      return { ...loaded, userMessageId: inserted.id };
    });

    maybeEmbedMessage(ctx, args.threadId, userMessageId);
    runLockedLoop(ctx, cfg, agentName, threadOwner, args.threadId);
    return {};
  }
);

export const regenerate_response = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    const owner = callerIdentity(ctx);
    const {
      cfg,
      agentName,
      owner: threadOwner,
    } = ctx.withTx(tx => {
      const loaded = loadLoopConfigOrThrow(tx, threadId, owner);

      const rows = threadMessagesAscending(tx, threadId);
      let lastUserMsgId: bigint | undefined;
      for (const r of rows) {
        if (r.role === 'user') lastUserMsgId = r.id;
      }
      if (lastUserMsgId === undefined) {
        throwSenderError(`agent.regenerate_no_user_message:${threadId}`);
      }

      for (const r of rows) {
        if (r.id > lastUserMsgId!) tx.db.message.delete(r);
      }

      tx.db.threadLock.insert({
        threadId,
        owner: loaded.owner,
        lockedAt: tx.timestamp,
        cancelRequested: false,
      });
      const threadRow = tx.db.thread.id.find(threadId);
      if (threadRow)
        tx.db.thread.id.update({ ...threadRow, updatedAt: tx.timestamp });
      return loaded;
    });

    runLockedLoop(ctx, cfg, agentName, threadOwner, threadId);
    return {};
  }
);

export const generate_thread_title = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    const owner = callerIdentity(ctx);
    const job = ctx.withTx(tx => {
      const threadRow = tx.db.thread.id.find(threadId);
      if (!threadRow) return null;
      if (!threadRow.owner.isEqual(owner)) {
        throwSenderError(`agent.not_thread_owner:${threadId}`);
      }
      if (threadRow.title != null && threadRow.title.length > 0) return null;

      const def = registry.agentDef(threadRow.agentName);
      if (!def) return null;
      const sumName = def.summarizerAgentName ?? threadRow.agentName;
      const sumDef = registry.agentDef(sumName);
      if (!sumDef) return null;

      const override = tx.db.agentOverride.agentName.find(sumName);
      const providerName = override?.provider ?? sumDef.defaultProvider;
      const provider = BUILT_IN_PROVIDERS[providerName];
      if (!provider) return null;
      const keyRow = tx.db.apiKey.provider.find(providerName);
      if (!keyRow) return null;

      const msgs = threadMessagesAscending(tx, threadId);
      const firstUser = msgs.find(m => m.role === 'user');
      if (!firstUser) return null;

      return {
        provider,
        apiKey: keyRow.key,
        model: override?.model ?? sumDef.defaultModel,
        retries: override?.retries ?? sumDef.defaultRetries,
        firstMessage: firstUser.content,
      };
    });
    if (!job) return {};

    const result = callChat(ctx.http, job.provider, {
      apiKey: job.apiKey,
      model: job.model,
      system:
        'You title chat conversations. The user will paste the opening message of ' +
        'a chat. You output a 3-5 word title describing the topic. ' +
        'CRITICAL: do not answer or respond to the message. Do not greet. ' +
        'Output the title and only the title. No quotes, no punctuation at the end.',
      messages: [
        {
          role: 'user',
          content: `Title for a chat that starts with this message:\n\n<message>\n${job.firstMessage}\n</message>`,
        },
      ],
      maxTokens: 30,
      retries: job.retries,
    });
    if (!result.ok || !result.response.text) {
      console.warn(
        `title gen failed: ${result.ok ? 'no text' : result.error.kind}`
      );
      return {};
    }

    const cleaned = result.response.text
      .trim()
      .replace(/^["']|["']$/g, '')
      .replace(/[.!?]+$/g, '')
      .slice(0, 80);

    ctx.withTx(tx => {
      const t2 = tx.db.thread.id.find(threadId);
      if (!t2 || (t2.title != null && t2.title.length > 0)) return;
      tx.db.thread.id.update({
        ...t2,
        title: cleaned,
        updatedAt: tx.timestamp,
      });
    });
    return {};
  }
);

export const thread_lock_sweep = spacetimedb.reducer(
  { onSchedule: threadLockSweeperTick },
  { arg: threadLockSweeperTick.rowType },
  (ctx, _arg) => {
    const secret = ctx.db.agentSecret.singleton.find(true);
    const thresholdSecs =
      secret?.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS;
    const thresholdMicros = BigInt(thresholdSecs) * ONE_SECOND_MICROS;

    const cutoffMicros = staleLockCutoffMicros(
      ctx.timestamp.microsSinceUnixEpoch,
      thresholdMicros
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
