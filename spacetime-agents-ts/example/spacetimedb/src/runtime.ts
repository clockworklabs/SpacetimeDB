import { makeAgentRegistry } from '@spacetimedb/agents';
import {
  callChat,
  type ChatMessage,
  type HttpLike,
} from '@spacetimedb/agents/openrouter';
import { BUILT_IN_PROVIDERS } from '@spacetimedb/agents/providers';
import {
  BUILT_IN_EMBEDDING_PROVIDERS,
  cosineSimilarity,
  topKByScore,
} from '@spacetimedb/agents/embeddings';
import { agents } from './agents';
import {
  runAgentLoop,
  type LoopConfig,
  type LoopMessage,
  type LoopTx,
} from './loop';
import {
  augmentSystemWithSummary,
  buildSummarizerUserContent,
  pickSummarizationCandidates,
} from './summarize';
import type { Tx } from './types';

type WriteCtx = Tx;

export const registry = makeAgentRegistry<Tx, typeof agents>(agents);

export interface ProcedureRuntimeContext {
  http: HttpLike;
  withTx: <R>(fn: (tx: WriteCtx) => R) => R;
}

function threadMessagesAscending(tx: WriteCtx, threadId: bigint) {
  const rows = [...tx.db.message.threadId.filter(threadId)];
  rows.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  return rows;
}

export function maybeEmbedMessage(
  ctx: ProcedureRuntimeContext,
  threadId: bigint,
  messageId: bigint
): void {
  const job = ctx.withTx(tx => {
    if (tx.db.messageEmbedding.messageId.find(messageId) != null) return null;
    const message = tx.db.message.id.find(messageId);
    if (!message) return null;
    const thread = tx.db.thread.id.find(threadId);
    if (!thread) return null;
    const definition = registry.agentDef(thread.agentName);
    if (!definition?.embeddingsProvider || !definition.embeddingsModel)
      return null;
    const provider =
      BUILT_IN_EMBEDDING_PROVIDERS[definition.embeddingsProvider];
    if (!provider) return null;
    const key = tx.db.apiKey.provider.find(definition.embeddingsProvider);
    if (!key) return null;
    return {
      provider,
      apiKey: key.key,
      model: definition.embeddingsModel,
      content: message.content,
      userId: message.userId,
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
      userId: job.userId,
      model: job.model,
      vector: result.vectors[0]!,
      createdAt: tx.timestamp,
    });
  });
}

function retrieveRag(ctx: ProcedureRuntimeContext, threadId: bigint): string[] {
  return ctx.withTx(tx => {
    const thread = tx.db.thread.id.find(threadId);
    if (!thread) return [];
    const definition = registry.agentDef(thread.agentName);
    if (!definition || definition.ragTopK <= 0) return [];

    const messages = threadMessagesAscending(tx, threadId);
    let queryMessage: (typeof messages)[number] | undefined;
    for (let index = messages.length - 1; index >= 0; index--) {
      if (messages[index]!.role === 'user') {
        queryMessage = messages[index];
        break;
      }
    }
    if (!queryMessage) return [];
    const queryEmbedding = tx.db.messageEmbedding.messageId.find(
      queryMessage.id
    );
    if (!queryEmbedding) return [];

    const override = tx.db.agentOverride.agentName.find(thread.agentName);
    const maxHistory =
      override?.maxHistoryMessages ?? definition.defaultMaxHistoryMessages;
    const windowStart = Math.max(0, messages.length - maxHistory);
    const inWindowIds = new Set(
      messages.slice(windowStart).map(message => message.id)
    );
    const candidates = [
      ...tx.db.messageEmbedding.threadId.filter(threadId),
    ].filter(
      embedding =>
        !inWindowIds.has(embedding.messageId) &&
        embedding.messageId !== queryMessage!.id
    );
    const top = topKByScore(
      candidates,
      embedding => cosineSimilarity(queryEmbedding.vector, embedding.vector),
      definition.ragTopK
    ).filter(result => result.score > 0);

    const snippets: string[] = [];
    for (const { item } of top) {
      const message = tx.db.message.id.find(item.messageId);
      if (message) snippets.push(`[${message.role}] ${message.content}`);
    }
    return snippets;
  });
}

function augmentSystemWithRag(
  base: string | undefined,
  snippets: string[]
): string | undefined {
  if (snippets.length === 0) return base;
  return `${base ?? ''}\n\n## Relevant earlier messages\n${snippets.join('\n---\n')}`.trim();
}

function runSummarization(
  ctx: ProcedureRuntimeContext,
  threadId: bigint
): void {
  const decision = ctx.withTx(tx => {
    const thread = tx.db.thread.id.find(threadId);
    if (!thread) return null;
    const definition = registry.agentDef(thread.agentName);
    if (!definition?.summarizerAgentName) return null;
    const summarizer = registry.agentDef(definition.summarizerAgentName);
    if (!summarizer) return null;

    const override = tx.db.agentOverride.agentName.find(thread.agentName);
    const maxHistory =
      override?.maxHistoryMessages ?? definition.defaultMaxHistoryMessages;
    const loopMessages: LoopMessage[] = threadMessagesAscending(
      tx,
      threadId
    ).map(message => ({
      id: message.id,
      threadId: message.threadId,
      role: message.role,
      content: message.content,
      toolCallsJson: message.toolCallsJson,
      toolCallId: message.toolCallId,
      isError: message.isError,
      promptTokens: message.promptTokens,
      completionTokens: message.completionTokens,
      attachments: [],
    }));
    const candidates = pickSummarizationCandidates(
      loopMessages,
      maxHistory,
      thread.summarizedThroughId ?? null
    );
    if (!candidates) return null;

    const summarizerOverride = tx.db.agentOverride.agentName.find(
      definition.summarizerAgentName
    );
    const providerName =
      summarizerOverride?.provider ?? summarizer.defaultProvider;
    const provider = BUILT_IN_PROVIDERS[providerName];
    const key = tx.db.apiKey.provider.find(providerName);
    if (!provider || !key) return null;
    return {
      provider,
      apiKey: key.key,
      model: summarizerOverride?.model ?? summarizer.defaultModel,
      systemPrompt:
        summarizerOverride?.systemPrompt ?? summarizer.defaultSystemPrompt,
      maxTokens: summarizerOverride?.maxTokens ?? summarizer.defaultMaxTokens,
      retries: summarizerOverride?.retries ?? summarizer.defaultRetries,
      existingSummary: thread.summary ?? null,
      newDropped: candidates.newDropped,
      lastNewId: candidates.lastNewId,
    };
  });
  if (!decision) return;

  const messages: ChatMessage[] = [
    {
      role: 'user',
      content: buildSummarizerUserContent(
        decision.existingSummary,
        decision.newDropped
      ),
    },
  ];
  const result = callChat(ctx.http, decision.provider, {
    apiKey: decision.apiKey,
    model: decision.model,
    system: decision.systemPrompt,
    messages,
    maxTokens: decision.maxTokens,
    retries: decision.retries,
  });
  if (!result.ok || !result.response.text) {
    console.warn(
      `summarization failed: ${result.ok ? 'no text in response' : result.error.kind}`
    );
    return;
  }

  ctx.withTx(tx => {
    const thread = tx.db.thread.id.find(threadId);
    if (!thread) return;
    tx.db.thread.id.update({
      ...thread,
      summary: result.response.text!,
      summarizedThroughId: decision.lastNewId,
      updatedAt: tx.timestamp,
    });
  });
}

const BASE64_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function bytesToBase64(bytes: ArrayLike<number>): string {
  let output = '';
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index]!;
    const second = index + 1 < bytes.length ? bytes[index + 1]! : 0;
    const third = index + 2 < bytes.length ? bytes[index + 2]! : 0;
    output += BASE64_ALPHABET[first >> 2];
    output += BASE64_ALPHABET[((first & 0x03) << 4) | (second >> 4)];
    output +=
      index + 1 < bytes.length
        ? BASE64_ALPHABET[((second & 0x0f) << 2) | (third >> 6)]
        : '=';
    output += index + 2 < bytes.length ? BASE64_ALPHABET[third & 0x3f] : '=';
  }
  return output;
}

function loadAttachments(
  tx: WriteCtx,
  messageId: bigint
): Array<{ mimeType: string; data: string }> {
  const rows = [...tx.db.messageAttachment.messageId.filter(messageId)];
  rows.sort((a, b) => a.ordinal - b.ordinal);
  const attachments: Array<{ mimeType: string; data: string }> = [];
  for (const row of rows) {
    const file = tx.db.files.file.id.find(row.fileId);
    const blob = tx.db.files.fileBlob.fileId.find(row.fileId);
    if (file && blob) {
      attachments.push({
        mimeType: file.mimeType,
        data: bytesToBase64(blob.bytes),
      });
    }
  }
  return attachments;
}

function adaptTx(
  tx: WriteCtx,
  agentName: string,
  userId: string,
  recordTokens: (tx: WriteCtx, userId: string, tokens: bigint) => void
): LoopTx {
  return {
    listMessages(threadId): LoopMessage[] {
      return threadMessagesAscending(tx, threadId).map(message => ({
        id: message.id,
        threadId: message.threadId,
        role: message.role,
        content: message.content,
        toolCallsJson: message.toolCallsJson,
        toolCallId: message.toolCallId,
        isError: message.isError,
        promptTokens: message.promptTokens,
        completionTokens: message.completionTokens,
        attachments:
          message.role === 'user' ? loadAttachments(tx, message.id) : [],
      }));
    },
    appendMessage(row): void {
      tx.db.message.insert({
        id: 0n,
        threadId: row.threadId,
        userId,
        role: row.role,
        content: row.content,
        toolCallsJson: row.toolCallsJson,
        toolCallId: row.toolCallId,
        isError: row.isError,
        promptTokens: row.promptTokens,
        completionTokens: row.completionTokens,
        createdAt: tx.timestamp,
      });
      if (
        row.role === 'assistant' &&
        (row.promptTokens != null || row.completionTokens != null)
      ) {
        const tokens = BigInt(
          (row.promptTokens ?? 0) + (row.completionTokens ?? 0)
        );
        if (tokens > 0n) recordTokens(tx, userId, tokens);
      }
    },
    bumpThread(threadId): void {
      const thread = tx.db.thread.id.find(threadId);
      if (thread)
        tx.db.thread.id.update({ ...thread, updatedAt: tx.timestamp });
    },
    invokeTool(name, inputJson) {
      return registry.invoke(agentName, tx, name, inputJson);
    },
    isCancelRequested(threadId): boolean {
      return tx.db.threadLock.threadId.find(threadId)?.cancelRequested ?? false;
    },
  };
}

export function runLockedLoop(
  ctx: ProcedureRuntimeContext,
  cfg: LoopConfig,
  agentName: string,
  userId: string,
  threadId: bigint,
  recordTokens: (tx: WriteCtx, userId: string, tokens: bigint) => void
): void {
  try {
    runSummarization(ctx, threadId);
    const ragSnippets = retrieveRag(ctx, threadId);
    const finalConfig = ctx.withTx(tx => {
      const thread = tx.db.thread.id.find(threadId);
      if (!thread) return cfg;
      const withSummary = augmentSystemWithSummary(
        cfg.systemPrompt,
        thread.summary ?? null
      );
      return {
        ...cfg,
        systemPrompt: augmentSystemWithRag(withSummary, ragSnippets),
      };
    });
    runAgentLoop({
      http: ctx.http,
      withTx: <R>(fn: (loopTx: LoopTx) => R): R =>
        ctx.withTx(tx => fn(adaptTx(tx, agentName, userId, recordTokens))),
      llmToolDefs: registry.llmToolDefsFor(agentName),
      cfg: finalConfig,
      threadId,
    });
  } finally {
    ctx.withTx(tx => {
      const lock = tx.db.threadLock.threadId.find(threadId);
      if (lock) tx.db.threadLock.delete(lock);
    });
  }
}
