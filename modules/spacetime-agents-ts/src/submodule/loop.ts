import { errors } from '../errors.js';
import {
  callChat,
  type ChatError,
  type ChatMessage,
  type ContentBlock,
  type HttpLike,
  type Provider,
  type ResponseFormat,
  type ToolCall,
  type ToolDefinition,
} from '../openrouter.js';

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
  /** Recalled summary and retrieved messages, sent as a leading user message. */
  context: string | undefined;
}

export interface LoopAttachment {
  mimeType: string;
  /** Base64-encoded bytes. */
  data: string;
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

// Only user messages carry attachments; the loop never appends them.
export type AppendMessageRow = Omit<LoopMessage, 'id' | 'attachments'>;

export interface LoopTx {
  listMessages(threadId: bigint): LoopMessage[];
  appendMessage(row: AppendMessageRow): void;
  bumpThread(threadId: bigint): void;
  invokeTool(
    name: string,
    inputJson: string
  ): { result: string; isError: boolean };
  isCancelRequested(threadId: bigint): boolean;
}

export interface RunAgentLoopOptions {
  http: HttpLike;
  withTx: <R>(fn: (tx: LoopTx) => R) => R;
  llmToolDefs: ToolDefinition[];
  cfg: LoopConfig;
  threadId: bigint;
}

function clip(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max)}...[truncated]`;
}

// Provider detail stays in the module log; the stored message is generic.
function chatErrorContent(error: ChatError): string {
  const detail =
    error.kind === 'http'
      ? `${error.status} ${error.body.slice(0, 500)}`
      : error.message;
  console.warn(`agents provider ${error.kind} error: ${detail}`);
  return error.kind === 'http'
    ? `${errors.providerError}:http_${error.status}`
    : `${errors.providerError}:${error.kind}`;
}

function userContent(row: LoopMessage): string | ContentBlock[] {
  if (row.attachments.length === 0) return row.content;
  const blocks: ContentBlock[] = [];
  if (row.content) blocks.push({ type: 'text', text: row.content });
  for (const attachment of row.attachments) {
    blocks.push({
      type: 'image',
      mimeType: attachment.mimeType,
      data: attachment.data,
    });
  }
  return blocks;
}

// Drops orphan tool rows whose assistant tool call fell outside the window.
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
  const messages: ChatMessage[] = [];
  const knownToolCallIds = new Set<string>();

  for (const row of window) {
    if (row.role === 'user') {
      messages.push({ role: 'user', content: userContent(row) });
    } else if (row.role === 'assistant') {
      let toolCalls: ToolCall[] | undefined;
      if (row.toolCallsJson != null) {
        try {
          toolCalls = JSON.parse(row.toolCallsJson) as ToolCall[];
        } catch {
          toolCalls = undefined;
        }
      }
      const message: ChatMessage = { role: 'assistant', content: row.content };
      if (toolCalls && toolCalls.length > 0) {
        message.tool_calls = toolCalls;
        for (const call of toolCalls) knownToolCallIds.add(call.id);
      }
      messages.push(message);
    } else if (row.role === 'tool') {
      const toolCallId = row.toolCallId ?? '';
      if (!knownToolCallIds.has(toolCallId)) continue;
      messages.push({
        role: 'tool',
        tool_call_id: toolCallId,
        content: row.content,
      });
    }
  }
  return messages;
}

function runOneTurn(options: RunAgentLoopOptions): boolean {
  const { http, withTx, llmToolDefs, cfg, threadId } = options;
  const cancelled = withTx(tx => {
    if (!tx.isCancelRequested(threadId)) return false;
    tx.appendMessage({
      threadId,
      role: 'assistant',
      content: errors.cancelled,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: true,
      promptTokens: undefined,
      completionTokens: undefined,
    });
    tx.bumpThread(threadId);
    return true;
  });
  if (cancelled) return false;

  const history = withTx(tx =>
    buildLlmMessages(tx, threadId, cfg.maxHistoryMessages)
  );
  const llmMessages: ChatMessage[] = cfg.context
    ? [{ role: 'user', content: cfg.context }, ...history]
    : history;
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
        content: chatErrorContent(result.error),
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
        const invocation = tx.invokeTool(
          call.function.name,
          call.function.arguments
        );
        tx.appendMessage({
          threadId,
          role: 'tool',
          content: clip(invocation.result, TOOL_RESULT_MAX),
          toolCallsJson: undefined,
          toolCallId: call.id,
          isError: invocation.isError,
          promptTokens: undefined,
          completionTokens: undefined,
        });
      }
    }
    tx.bumpThread(threadId);
  });
  return hasToolCalls && finishReason === 'tool_calls';
}

export function runAgentLoop(options: RunAgentLoopOptions): void {
  for (let turn = 0; turn < options.cfg.maxTurns; turn++) {
    if (!runOneTurn(options)) return;
  }
  options.withTx(tx =>
    tx.appendMessage({
      threadId: options.threadId,
      role: 'assistant',
      content: `${errors.maxTurnsExceeded}:${options.cfg.maxTurns}`,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: true,
      promptTokens: undefined,
      completionTokens: undefined,
    })
  );
}
