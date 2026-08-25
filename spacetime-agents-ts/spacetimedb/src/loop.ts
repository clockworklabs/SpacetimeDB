import {
  callChat,
  type ChatMessage,
  type HttpLike,
  type Provider,
  type ResponseFormat,
  type ToolCall,
  type ToolDefinition,
} from '@spacetimedb/agents/openrouter';

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
}

export type AppendMessageRow = Omit<LoopMessage, 'id'>;

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

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max)}...`;
}

function formatChatError(error: {
  kind: string;
  status?: number;
  message?: string;
  body?: string;
}): string {
  switch (error.kind) {
    case 'http':
      return `agent.provider_http:${error.status}:${truncate(error.body ?? '', 500)}`;
    case 'transport':
      return `agent.provider_transport:${error.message ?? 'unknown'}`;
    case 'parse':
      return `agent.provider_parse:${error.message ?? 'unknown'}`;
    default:
      return `agent.provider_error:${error.kind}`;
  }
}

function buildLlmMessages(
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
      messages.push({ role: 'user', content: row.content });
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
      content: 'agent.cancelled',
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
      content: `agent.max_turns_exceeded:${options.cfg.maxTurns}`,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: true,
      promptTokens: undefined,
      completionTokens: undefined,
    })
  );
}
