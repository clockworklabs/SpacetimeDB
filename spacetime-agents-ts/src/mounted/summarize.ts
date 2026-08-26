import type { LoopMessage } from './loop';

export function pickSummarizationCandidates(
  messages: LoopMessage[],
  maxHistoryMessages: number,
  summarizedThroughId: bigint | null
): { newDropped: LoopMessage[]; lastNewId: bigint } | null {
  if (messages.length <= maxHistoryMessages) return null;
  const dropped = messages.slice(0, messages.length - maxHistoryMessages);
  const newDropped =
    summarizedThroughId == null
      ? dropped
      : dropped.filter(message => message.id > summarizedThroughId);
  if (newDropped.length === 0) return null;
  return { newDropped, lastNewId: newDropped[newDropped.length - 1]!.id };
}

export function formatMessagesForSummarizer(messages: LoopMessage[]): string {
  const lines: string[] = [];
  for (const message of messages) {
    if (message.role === 'user') {
      lines.push(`User: ${message.content}`);
    } else if (message.role === 'assistant') {
      if (message.toolCallsJson != null) {
        try {
          const calls = JSON.parse(message.toolCallsJson) as Array<{
            function?: { name?: string; arguments?: string };
          }>;
          for (const call of calls) {
            lines.push(
              `[Assistant called tool ${call.function?.name ?? '?'}(${call.function?.arguments ?? ''})]`
            );
          }
        } catch {
          // Preserve the assistant text when stored tool metadata is malformed.
        }
      }
      if (message.content) lines.push(`Assistant: ${message.content}`);
    } else if (message.role === 'tool') {
      lines.push(`[Tool result: ${message.content}]`);
    }
  }
  return lines.join('\n');
}

export function buildSummarizerUserContent(
  existingSummary: string | null,
  newDropped: LoopMessage[]
): string {
  const formatted = formatMessagesForSummarizer(newDropped);
  if (existingSummary) {
    return (
      `Existing summary:\n${existingSummary}\n\n` +
      `Additional messages to fold into the summary:\n${formatted}`
    );
  }
  return `Messages to summarize:\n${formatted}`;
}

export function augmentSystemWithSummary(
  baseSystem: string | undefined,
  summary: string | null
): string | undefined {
  if (!summary) return baseSystem;
  return `${baseSystem ?? ''}\n\n## Summary of earlier conversation\n${summary}`.trim();
}
