import type { LoopMessage } from './loop';

export function pickSummarizationCandidates(
  messages: LoopMessage[], // ascending by id
  maxHistoryMessages: number,
  summarizedThroughId: bigint | null
): { newDropped: LoopMessage[]; lastNewId: bigint } | null {
  if (messages.length <= maxHistoryMessages) return null;

  const dropCount = messages.length - maxHistoryMessages;
  const dropped = messages.slice(0, dropCount);

  const newDropped =
    summarizedThroughId == null
      ? dropped
      : dropped.filter(m => m.id > summarizedThroughId);

  if (newDropped.length === 0) return null;
  return { newDropped, lastNewId: newDropped[newDropped.length - 1].id };
}

export function formatMessagesForSummarizer(messages: LoopMessage[]): string {
  const lines: string[] = [];
  for (const m of messages) {
    if (m.role === 'user') {
      lines.push(`User: ${m.content}`);
    } else if (m.role === 'assistant') {
      if (m.toolCallsJson != null) {
        try {
          const calls = JSON.parse(m.toolCallsJson) as Array<{
            function?: { name?: string; arguments?: string };
          }>;
          for (const c of calls) {
            const name = c.function?.name ?? '?';
            const args = c.function?.arguments ?? '';
            lines.push(`[Assistant called tool ${name}(${args})]`);
          }
        } catch {
          /* malformed */
        }
        if (m.content) lines.push(`Assistant: ${m.content}`);
      } else {
        lines.push(`Assistant: ${m.content}`);
      }
    } else if (m.role === 'tool') {
      lines.push(`[Tool result: ${m.content}]`);
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
  if (summary == null || summary.length === 0) return baseSystem;
  const base = baseSystem ?? '';
  return `${base}\n\n## Summary of earlier conversation\n${summary}`.trim();
}
