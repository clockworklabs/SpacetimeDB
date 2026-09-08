import {
  callChat,
  type ChatMessage,
  type ContentBlock,
  type ToolCall,
  type HttpLike,
  type ToolDefinition,
  type ResponseFormat,
  type Provider,
} from '@spacetimedb/agents/openrouter';
import type { InvokeResult } from '@spacetimedb/agents';

export const USER_CONTENT_MAX = 32_000;
export const TOOL_RESULT_MAX = 64_000;

export interface LoopConfig {
  provider: Provider;
  apiKey: string;
  model: string;
  systemPrompt: string | undefined;
  maxTurns: number;
  maxHistoryMessages: number;
  maxTokens: number | undefined;
  retries: number;
  responseFormat: ResponseFormat | undefined;
}

export interface LoopAttachment {
  mimeType: string;
  data: string; // base64 (provider HTTP APIs want strings)
}

export interface LoopMessage {
  id: bigint;
  threadId: bigint;
  role: string;
  content: string;
  toolCallsJson: string | undefined;
  toolCallId: string | undefined;
  isError: boolean;
  promptTokens: number | undefined;
  completionTokens: number | undefined;
  attachments: LoopAttachment[];
}

// Only user messages carry attachments; the loop itself never appends them.
export type AppendMessageRow = Omit<LoopMessage, 'id' | 'attachments'>;

export interface LoopTx {
  listMessages(threadId: bigint): LoopMessage[];
  appendMessage(row: AppendMessageRow): void;
  bumpThread(threadId: bigint): void;
  invokeTool(name: string, inputJson: string): InvokeResult;
  isCancelRequested(threadId: bigint): boolean;
}

export type WithTx = <R>(fn: (tx: LoopTx) => R) => R;

export interface RunAgentLoopOpts {
  http: HttpLike;
  withTx: WithTx;
  llmToolDefs: ToolDefinition[];
  cfg: LoopConfig;
  threadId: bigint;
}

function runOneTurn(opts: RunAgentLoopOpts): boolean {
  const { http, withTx, llmToolDefs, cfg, threadId } = opts;

  const cancelled = withTx(tx => {
    if (tx.isCancelRequested(threadId)) {
      tx.appendMessage({
        threadId,
        role: 'assistant',
        content: 'agent.cancelled',
        toolCallsJson: undefined,
        toolCallId: undefined,
        isError: true,
        promptTokens: undefined,
        completionTokens: undefined,
      });
      tx.bumpThread(threadId);
      return true;
    }
    return false;
  });
  if (cancelled) return false;

  const llmMessages = withTx(tx =>
    buildLlmMessages(tx, threadId, cfg.maxHistoryMessages)
  );

  const result = callChat(http, cfg.provider, {
    apiKey: cfg.apiKey,
    model: cfg.model,
    system: cfg.systemPrompt,
    messages: llmMessages,
    tools: llmToolDefs,
    maxTokens: cfg.maxTokens,
    responseFormat: cfg.responseFormat,
    retries: cfg.retries,
  });

  if (!result.ok) {
    withTx(tx =>
      tx.appendMessage({
        threadId,
        role: 'assistant',
        content: formatChatError(result.error),
        toolCallsJson: undefined,
        toolCallId: undefined,
        isError: true,
        promptTokens: undefined,
        completionTokens: undefined,
      })
    );
    return false;
  }

  const { text, toolCalls, finishReason, usage } = result.response;
  const hasToolCalls = toolCalls.length > 0;

  withTx(tx => {
    tx.appendMessage({
      threadId,
      role: 'assistant',
      content: text ?? '',
      toolCallsJson: hasToolCalls ? JSON.stringify(toolCalls) : undefined,
      toolCallId: undefined,
      isError: false,
      promptTokens: usage.promptTokens > 0 ? usage.promptTokens : undefined,
      completionTokens:
        usage.completionTokens > 0 ? usage.completionTokens : undefined,
    });

    if (hasToolCalls) {
      for (const call of toolCalls) {
        const inv = tx.invokeTool(call.function.name, call.function.arguments);
        tx.appendMessage({
          threadId,
          role: 'tool',
          content: clip(inv.result, TOOL_RESULT_MAX),
          toolCallsJson: undefined,
          toolCallId: call.id,
          isError: inv.isError,
          promptTokens: undefined,
          completionTokens: undefined,
        });
      }
    }

    tx.bumpThread(threadId);
  });

  return hasToolCalls && finishReason === 'tool_calls';
}

export function runAgentLoop(opts: RunAgentLoopOpts): void {
  for (let turn = 0; turn < opts.cfg.maxTurns; turn++) {
    if (!runOneTurn(opts)) return;
  }
  opts.withTx(tx =>
    tx.appendMessage({
      threadId: opts.threadId,
      role: 'assistant',
      content: `agent.max_turns_exceeded:${opts.cfg.maxTurns}`,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: true,
      promptTokens: undefined,
      completionTokens: undefined,
    })
  );
}

// Drops orphan tool rows whose assistant tool_call fell outside the window.
export function buildLlmMessages(
  tx: LoopTx,
  threadId: bigint,
  maxHistoryMessages: number
): ChatMessage[] {
  const all = tx.listMessages(threadId);
  const window =
    maxHistoryMessages > 0 && all.length > maxHistoryMessages
      ? all.slice(all.length - maxHistoryMessages)
      : all;

  const out: ChatMessage[] = [];
  const knownToolCallIds = new Set<string>();

  for (const row of window) {
    if (row.role === 'user') {
      out.push({ role: 'user', content: userContent(row) });
    } else if (row.role === 'assistant') {
      let toolCalls: ToolCall[] | undefined;
      if (row.toolCallsJson != null) {
        try {
          toolCalls = JSON.parse(row.toolCallsJson) as ToolCall[];
        } catch {
          toolCalls = undefined;
        }
      }
      const msg: ChatMessage = { role: 'assistant', content: row.content };
      if (toolCalls && toolCalls.length > 0) {
        msg.tool_calls = toolCalls;
        for (const c of toolCalls) knownToolCallIds.add(c.id);
      }
      out.push(msg);
    } else if (row.role === 'tool') {
      const tcid = row.toolCallId ?? '';
      if (!knownToolCallIds.has(tcid)) continue;
      out.push({ role: 'tool', tool_call_id: tcid, content: row.content });
    }
  }
  return out;
}

function userContent(row: LoopMessage): string | ContentBlock[] {
  if (row.attachments.length === 0) return row.content;
  const blocks: ContentBlock[] = [];
  if (row.content) blocks.push({ type: 'text', text: row.content });
  for (const a of row.attachments) {
    blocks.push({ type: 'image', mimeType: a.mimeType, data: a.data });
  }
  return blocks;
}

export function formatChatError(err: {
  kind: string;
  status?: number;
  message?: string;
  body?: string;
}): string {
  switch (err.kind) {
    case 'http':
      return `agent.provider_http:${err.status}:${truncate(err.body ?? '', 500)}`;
    case 'transport':
      return `agent.provider_transport:${err.message ?? 'unknown'}`;
    case 'parse':
      return `agent.provider_parse:${err.message ?? 'unknown'}`;
    default:
      return `agent.provider_error:${err.kind}`;
  }
}

function truncate(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max) + '…';
}

function clip(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max) + '…[truncated]';
}
