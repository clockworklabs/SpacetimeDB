import type { Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import { makeAgentRegistry, type AgentDefinition } from '../agent';
import { callChat, type HttpLike } from '../openrouter';
import { BUILT_IN_PROVIDERS } from '../providers';
import {
  BUILT_IN_EMBEDDING_PROVIDERS,
  cosineSimilarity,
  topKByScore,
} from '../embeddings';
import { errors } from '../errors';
import {
  DEFAULT_STALE_LOCK_THRESHOLD_SECS,
  isAgentAdmin,
  type AgentsTx,
} from './index';
import {
  runAgentLoop,
  USER_CONTENT_MAX,
  type LoopAttachment,
  type LoopConfig,
  type LoopMessage,
  type LoopTx,
} from './loop';
import {
  buildContextMessage,
  buildSummarizerUserContent,
  pickSummarizationCandidates,
} from './summarize';

const DEFAULT_MAX_THREADS_PER_OWNER = 100;
const TITLE_SYSTEM_PROMPT =
  'You title chat conversations. The user will paste the opening message of ' +
  'a chat. You output a 3-5 word title describing the topic. ' +
  'CRITICAL: do not answer or respond to the message. Do not greet. ' +
  'Output the title and only the title. No quotes, no punctuation at the end.';

export interface AgentRun {
  owner: string;
  threadId: bigint;
  agentName: string;
}

export interface AgentUsage extends AgentRun {
  promptTokens: number;
  completionTokens: number;
}

export interface AgentInfo {
  name: string;
  model: string;
  models: string[];
}

export interface AgentsClientConfig<Tx> {
  /** Agent definitions by runtime name. Threads store this name. */
  agents: Record<string, AgentDefinition>;
  /** Returns the Agents submodule view of a host transaction, such as `tx => tx.as.agents`. */
  submodule(tx: Tx): AgentsTx;
  /** Runs in the transaction that starts each model run. Throw to reject the run. */
  beforeRun?(tx: Tx, run: AgentRun): void;
  /** Receives token usage reported by each model response. */
  onUsage?(tx: Tx, usage: AgentUsage): void;
  /** Returns image attachments for a user message. */
  attachments?(tx: Tx, messageId: bigint): LoopAttachment[];
  /** Defaults to 100. */
  maxThreadsPerOwner?: number;
}

export interface AgentsProcedureCtx<Tx> {
  http: HttpLike;
  withTx<R>(body: (tx: Tx) => R): R;
}

type Db = AgentsTx['db'];
type ThreadRow = NonNullable<ReturnType<Db['thread']['id']['find']>>;
type MessageRow = NonNullable<ReturnType<Db['message']['id']['find']>>;

function fail(code: string, detail?: unknown): never {
  throw new SenderError(detail === undefined ? code : `${code}:${detail}`);
}

function ownedThread(db: Db, threadId: bigint, owner: string): ThreadRow {
  const row = db.thread.id.find(threadId);
  if (!row) fail(errors.threadNotFound, threadId);
  if (row.owner !== owner) fail(errors.notThreadOwner, threadId);
  return row;
}

function messagesAscending(db: Db, threadId: bigint): MessageRow[] {
  return [...db.message.threadId.filter(threadId)].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  );
}

/**
 * Builds thread and model-run operations for host reducers and procedures.
 * The host authorizes callers and chooses each `owner` key.
 */
export function client<Tx extends { timestamp: Timestamp }>(
  config: AgentsClientConfig<Tx>
) {
  const registry = makeAgentRegistry<Tx, Record<string, AgentDefinition>>(
    config.agents
  );
  for (const name of registry.names()) {
    const summarizer = registry.agentDef(name)!.summarizerAgentName;
    if (summarizer !== undefined && !registry.has(summarizer)) {
      throw new Error(
        `agent '${name}' names unknown summarizer '${summarizer}'`
      );
    }
  }
  const maxThreads = config.maxThreadsPerOwner ?? DEFAULT_MAX_THREADS_PER_OWNER;
  if (!Number.isInteger(maxThreads) || maxThreads <= 0) {
    throw new Error('maxThreadsPerOwner must be a positive integer');
  }
  const db = (tx: Tx): Db => config.submodule(tx).db;

  // Effective settings: operator override, then agent code default.
  function settings(tx: Tx, agentName: string) {
    const def = registry.agentDef(agentName);
    if (!def) fail(errors.unknownAgent, agentName);
    const override = db(tx).agentOverride.agentName.find(agentName);
    const providerName = override?.provider ?? def.defaultProvider;
    return {
      def,
      providerName,
      provider: BUILT_IN_PROVIDERS[providerName],
      apiKey: db(tx).apiKey.provider.find(providerName)?.key,
      model: override?.model,
      systemPrompt: override?.systemPrompt ?? def.defaultSystemPrompt,
      maxTurns: override?.maxTurns ?? def.defaultMaxTurns,
      maxHistoryMessages:
        override?.maxHistoryMessages ?? def.defaultMaxHistoryMessages,
      maxTokens: override?.maxTokens ?? def.defaultMaxTokens,
      retries: override?.retries ?? def.defaultRetries,
    };
  }

  function recordUsage(
    tx: Tx,
    run: AgentRun,
    usage: { promptTokens: number; completionTokens: number }
  ): void {
    if (usage.promptTokens + usage.completionTokens > 0) {
      config.onUsage?.(tx, { ...run, ...usage });
    }
  }

  // Validates ownership and configuration, then takes the thread lock.
  function beginRun(tx: Tx, threadId: bigint, owner: string) {
    const thread = ownedThread(db(tx), threadId, owner);
    const s = settings(tx, thread.agentName);
    if (db(tx).threadLock.threadId.find(threadId) != null) {
      fail(errors.threadBusy, threadId);
    }
    const run = { owner, threadId, agentName: thread.agentName };
    config.beforeRun?.(tx, run);
    if (!s.provider) fail(errors.unknownProvider, s.providerName);
    if (s.apiKey === undefined) fail(errors.noApiKey, s.providerName);

    const threadModel =
      thread.modelOverride !== undefined &&
      s.def.models.includes(thread.modelOverride)
        ? thread.modelOverride
        : undefined;
    const cfg: LoopConfig = {
      provider: s.provider,
      apiKey: s.apiKey,
      model: s.model ?? threadModel ?? s.def.defaultModel,
      systemPrompt: s.systemPrompt,
      maxTurns: s.maxTurns,
      maxHistoryMessages: s.maxHistoryMessages,
      maxTokens: s.maxTokens,
      retries: s.retries,
      responseFormat: s.def.defaultResponseFormat,
      context: undefined,
    };
    db(tx).threadLock.insert({
      threadId,
      owner,
      lockedAt: tx.timestamp,
      cancelRequested: false,
    });
    db(tx).thread.id.update({ ...thread, updatedAt: tx.timestamp });
    return { run, cfg };
  }

  function embedMessage(
    ctx: AgentsProcedureCtx<Tx>,
    run: AgentRun,
    messageId: bigint
  ): void {
    const job = ctx.withTx(tx => {
      const def = registry.agentDef(run.agentName);
      if (!def?.embeddingsProvider || !def.embeddingsModel) return null;
      const provider = BUILT_IN_EMBEDDING_PROVIDERS[def.embeddingsProvider];
      const key = db(tx).apiKey.provider.find(def.embeddingsProvider);
      const message = db(tx).message.id.find(messageId);
      if (!provider || !key || !message) return null;
      return {
        provider,
        apiKey: key.key,
        model: def.embeddingsModel,
        content: message.content,
      };
    });
    if (!job) return;

    const result = job.provider.embed(ctx.http, job.apiKey, job.model, [
      job.content,
    ]);
    if (!result.ok || result.vectors.length === 0) {
      console.warn(
        `agents embedding failed: ${result.ok ? 'no vectors' : result.error.kind}`
      );
      return;
    }
    ctx.withTx(tx => {
      if (db(tx).messageEmbedding.messageId.find(messageId) != null) return;
      db(tx).messageEmbedding.insert({
        messageId,
        threadId: run.threadId,
        owner: run.owner,
        model: job.model,
        vector: result.vectors[0],
        createdAt: tx.timestamp,
      });
    });
  }

  function retrieveSnippets(tx: Tx, run: AgentRun): string[] {
    const s = settings(tx, run.agentName);
    if (s.def.ragTopK <= 0) return [];
    const messages = messagesAscending(db(tx), run.threadId);
    const query = messages.findLast(message => message.role === 'user');
    if (!query) return [];
    const queryEmbedding = db(tx).messageEmbedding.messageId.find(query.id);
    if (!queryEmbedding) return [];

    const inWindow = new Set(
      messages.slice(-s.maxHistoryMessages).map(message => message.id)
    );
    const candidates = [
      ...db(tx).messageEmbedding.threadId.filter(run.threadId),
    ].filter(embedding => !inWindow.has(embedding.messageId));
    const top = topKByScore(
      candidates,
      embedding => cosineSimilarity(queryEmbedding.vector, embedding.vector),
      s.def.ragTopK
    ).filter(result => result.score > 0);

    const snippets: string[] = [];
    for (const { item } of top) {
      const message = db(tx).message.id.find(item.messageId);
      if (message) snippets.push(`[${message.role}] ${message.content}`);
    }
    return snippets;
  }

  function summarize(ctx: AgentsProcedureCtx<Tx>, run: AgentRun): void {
    const job = ctx.withTx(tx => {
      const def = registry.agentDef(run.agentName);
      if (!def?.summarizerAgentName) return null;
      const thread = db(tx).thread.id.find(run.threadId);
      if (!thread) return null;
      const candidates = pickSummarizationCandidates(
        messagesAscending(db(tx), run.threadId).map(toLoopMessage),
        settings(tx, run.agentName).maxHistoryMessages,
        thread.summarizedThroughId ?? null
      );
      if (!candidates) return null;
      const s = settings(tx, def.summarizerAgentName);
      if (!s.provider || s.apiKey === undefined) return null;
      return {
        s: { ...s, provider: s.provider, apiKey: s.apiKey },
        existingSummary: thread.summary ?? null,
        ...candidates,
      };
    });
    if (!job) return;

    const result = callChat(ctx.http, job.s.provider, {
      apiKey: job.s.apiKey,
      model: job.s.model ?? job.s.def.defaultModel,
      system: job.s.systemPrompt,
      messages: [
        {
          role: 'user',
          content: buildSummarizerUserContent(
            job.existingSummary,
            job.newDropped
          ),
        },
      ],
      maxTokens: job.s.maxTokens,
      retries: job.s.retries,
    });
    if (!result.ok || !result.response.text) {
      console.warn(
        `agents summarization failed: ${result.ok ? 'no text in response' : result.error.kind}`
      );
      return;
    }
    const summary = result.response.text;
    ctx.withTx(tx => {
      recordUsage(tx, run, result.response.usage);
      const thread = db(tx).thread.id.find(run.threadId);
      if (!thread) return;
      db(tx).thread.id.update({
        ...thread,
        summary,
        summarizedThroughId: job.lastNewId,
        updatedAt: tx.timestamp,
      });
    });
  }

  function loopTx(tx: Tx, run: AgentRun): LoopTx {
    return {
      listMessages(threadId) {
        return messagesAscending(db(tx), threadId).map(message => ({
          ...toLoopMessage(message),
          attachments:
            message.role === 'user' && config.attachments
              ? config.attachments(tx, message.id)
              : [],
        }));
      },
      appendMessage(row) {
        db(tx).message.insert({
          ...row,
          id: 0n,
          owner: run.owner,
          createdAt: tx.timestamp,
        });
        recordUsage(tx, run, {
          promptTokens: row.promptTokens ?? 0,
          completionTokens: row.completionTokens ?? 0,
        });
      },
      bumpThread(threadId) {
        const thread = db(tx).thread.id.find(threadId);
        if (thread) {
          db(tx).thread.id.update({ ...thread, updatedAt: tx.timestamp });
        }
      },
      invokeTool(name, inputJson) {
        return registry.invoke(run.agentName, tx, name, inputJson);
      },
      isCancelRequested(threadId) {
        return (
          db(tx).threadLock.threadId.find(threadId)?.cancelRequested ?? false
        );
      },
    };
  }

  // Releases the thread lock however the run ends.
  function runLocked(
    ctx: AgentsProcedureCtx<Tx>,
    started: { run: AgentRun; cfg: LoopConfig },
    userMessageId?: bigint
  ): void {
    const { run, cfg } = started;
    try {
      if (userMessageId !== undefined) embedMessage(ctx, run, userMessageId);
      summarize(ctx, run);
      const context = ctx.withTx(tx =>
        buildContextMessage(
          db(tx).thread.id.find(run.threadId)?.summary,
          retrieveSnippets(tx, run)
        )
      );
      runAgentLoop({
        http: ctx.http,
        withTx: fn => ctx.withTx(tx => fn(loopTx(tx, run))),
        llmToolDefs: registry.llmToolDefsFor(run.agentName),
        cfg: { ...cfg, context },
        threadId: run.threadId,
      });
    } finally {
      ctx.withTx(tx => db(tx).threadLock.threadId.delete(run.threadId));
    }
  }

  return {
    startThread(
      tx: Tx,
      args: {
        owner: string;
        agentName: string;
        title?: string;
        metadata?: string;
      }
    ): bigint {
      if (!registry.has(args.agentName)) {
        fail(errors.unknownAgent, args.agentName);
      }
      if ([...db(tx).thread.owner.filter(args.owner)].length >= maxThreads) {
        fail(errors.tooManyThreads, maxThreads);
      }
      return db(tx).thread.insert({
        id: 0n,
        owner: args.owner,
        agentName: args.agentName,
        title: args.title,
        modelOverride: undefined,
        metadata: args.metadata,
        summary: undefined,
        summarizedThroughId: undefined,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      }).id;
    },

    /** `modelOverride` must be listed in the agent's `models`. */
    updateThread(
      tx: Tx,
      args: {
        owner: string;
        threadId: bigint;
        title?: string;
        modelOverride?: string;
        metadata?: string;
        clearTitle?: boolean;
        clearModelOverride?: boolean;
        clearMetadata?: boolean;
      }
    ): void {
      const thread = ownedThread(db(tx), args.threadId, args.owner);
      if (
        args.modelOverride !== undefined &&
        !registry
          .agentDef(thread.agentName)
          ?.models.includes(args.modelOverride)
      ) {
        fail(errors.modelNotAllowed, args.modelOverride);
      }
      db(tx).thread.id.update({
        ...thread,
        title: args.clearTitle ? undefined : (args.title ?? thread.title),
        modelOverride: args.clearModelOverride
          ? undefined
          : (args.modelOverride ?? thread.modelOverride),
        metadata: args.clearMetadata
          ? undefined
          : (args.metadata ?? thread.metadata),
        updatedAt: tx.timestamp,
      });
    },

    deleteThread(tx: Tx, args: { owner: string; threadId: bigint }): void {
      ownedThread(db(tx), args.threadId, args.owner);
      if (db(tx).threadLock.threadId.find(args.threadId) != null) {
        fail(errors.threadBusy, args.threadId);
      }
      for (const row of [
        ...db(tx).messageEmbedding.threadId.filter(args.threadId),
      ]) {
        db(tx).messageEmbedding.delete(row);
      }
      for (const row of [...db(tx).message.threadId.filter(args.threadId)]) {
        db(tx).message.delete(row);
      }
      db(tx).thread.id.delete(args.threadId);
    },

    requestCancel(tx: Tx, args: { owner: string; threadId: bigint }): void {
      ownedThread(db(tx), args.threadId, args.owner);
      const lock = db(tx).threadLock.threadId.find(args.threadId);
      if (!lock) fail(errors.threadNotRunning, args.threadId);
      if (!lock.cancelRequested) {
        db(tx).threadLock.threadId.update({ ...lock, cancelRequested: true });
      }
    },

    /**
     * Stores a user message and runs the agent. `onInsert` runs in the same
     * transaction, for example to attach files. Empty content requires it.
     */
    sendMessage(
      ctx: AgentsProcedureCtx<Tx>,
      args: {
        owner: string;
        threadId: bigint;
        content: string;
        onInsert?: (tx: Tx, messageId: bigint) => void;
      }
    ): void {
      if (args.content.length === 0 && !args.onInsert) {
        fail(errors.emptyMessage);
      }
      const content =
        args.content.length > USER_CONTENT_MAX
          ? `${args.content.slice(0, USER_CONTENT_MAX)}...[truncated]`
          : args.content;
      const { started, messageId } = ctx.withTx(tx => {
        const started = beginRun(tx, args.threadId, args.owner);
        const messageId = db(tx).message.insert({
          id: 0n,
          threadId: args.threadId,
          owner: args.owner,
          role: 'user',
          content,
          toolCallsJson: undefined,
          toolCallId: undefined,
          isError: false,
          promptTokens: undefined,
          completionTokens: undefined,
          createdAt: tx.timestamp,
        }).id;
        args.onInsert?.(tx, messageId);
        return { started, messageId };
      });
      runLocked(ctx, started, messageId);
    },

    /** Deletes replies after the last user message and runs the agent again. */
    regenerateResponse(
      ctx: AgentsProcedureCtx<Tx>,
      args: { owner: string; threadId: bigint }
    ): void {
      const started = ctx.withTx(tx => {
        const started = beginRun(tx, args.threadId, args.owner);
        const messages = messagesAscending(db(tx), args.threadId);
        const lastUser = messages.findLast(message => message.role === 'user');
        if (!lastUser) fail(errors.regenerateNoUserMessage, args.threadId);
        for (const message of messages) {
          if (message.id > lastUser.id) db(tx).message.delete(message);
        }
        return started;
      });
      runLocked(ctx, started);
    },

    /** Titles an untitled thread with its summarizer agent, or its own agent. */
    generateThreadTitle(
      ctx: AgentsProcedureCtx<Tx>,
      args: { owner: string; threadId: bigint }
    ): void {
      const job = ctx.withTx(tx => {
        const thread = ownedThread(db(tx), args.threadId, args.owner);
        if (thread.title) return null;
        const firstUser = messagesAscending(db(tx), args.threadId).find(
          message => message.role === 'user'
        );
        if (!firstUser) return null;
        const run = {
          owner: args.owner,
          threadId: args.threadId,
          agentName: thread.agentName,
        };
        const s = settings(
          tx,
          registry.agentDef(thread.agentName)?.summarizerAgentName ??
            thread.agentName
        );
        if (!s.provider || s.apiKey === undefined) return null;
        config.beforeRun?.(tx, run);
        return {
          run,
          provider: s.provider,
          apiKey: s.apiKey,
          model: s.model ?? s.def.defaultModel,
          retries: s.retries,
          firstMessage: firstUser.content,
        };
      });
      if (!job) return;

      const result = callChat(ctx.http, job.provider, {
        apiKey: job.apiKey,
        model: job.model,
        system: TITLE_SYSTEM_PROMPT,
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
          `agents title generation failed: ${result.ok ? 'no text' : result.error.kind}`
        );
        return;
      }
      const title = result.response.text
        .trim()
        .replace(/^["']|["']$/g, '')
        .replace(/[.!?]+$/g, '')
        .slice(0, 80);
      ctx.withTx(tx => {
        recordUsage(tx, job.run, result.response.usage);
        const thread = db(tx).thread.id.find(args.threadId);
        if (!thread || thread.title) return;
        db(tx).thread.id.update({ ...thread, title, updatedAt: tx.timestamp });
      });
    },

    /** Agents with their effective model and selectable models, plus configured providers. */
    status(tx: Tx): {
      agents: AgentInfo[];
      configuredProviders: string[];
      staleLockThresholdSecs: number;
    } {
      return {
        agents: registry.names().map(name => {
          const s = settings(tx, name);
          return {
            name,
            model: s.model ?? s.def.defaultModel,
            models: s.model === undefined ? s.def.models : [],
          };
        }),
        configuredProviders: [...db(tx).apiKey.iter()]
          .map(row => row.provider)
          .sort(),
        staleLockThresholdSecs:
          db(tx).agentConfig.singleton.find(true)?.staleLockThresholdSecs ??
          DEFAULT_STALE_LOCK_THRESHOLD_SECS,
      };
    },

    isAdmin(tx: Tx): boolean {
      return isAgentAdmin(config.submodule(tx));
    },
  };
}

function toLoopMessage(row: MessageRow): LoopMessage {
  return {
    id: row.id,
    threadId: row.threadId,
    role: row.role,
    content: row.content,
    toolCallsJson: row.toolCallsJson,
    toolCallId: row.toolCallId,
    isError: row.isError,
    promptTokens: row.promptTokens,
    completionTokens: row.completionTokens,
    attachments: [],
  };
}
