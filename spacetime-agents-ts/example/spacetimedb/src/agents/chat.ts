import { defineAgent } from '@spacetimedb/agents';
import getTime from '../tools/getTime';

export default defineAgent({
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
  },
});
