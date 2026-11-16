import { defineAgent } from '@spacetimedb/agents';

export default defineAgent({
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
