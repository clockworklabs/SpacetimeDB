export {
  agentTool,
  makeAgentDispatch,
  defineAgent,
  makeAgentRegistry,
  typeBuilderToJsonSchema,
} from './agent.js';
export type {
  AgentTool,
  AgentDefinition,
  AgentRegistry,
  InvokeResult,
  ToolMap,
} from './agent.js';

export { callChat, isRetryableError } from './openrouter.js';
export type {
  HttpLike,
  ChatMessage,
  ContentBlock,
  ToolCall,
  ToolDefinition,
  ChatRequest,
  ChatResponse,
  ChatError,
  ChatResult,
  ResponseFormat,
  Provider,
} from './openrouter.js';

export {
  openRouterProvider,
  openAiProvider,
  anthropicProvider,
  BUILT_IN_PROVIDERS,
} from './providers.js';

export {
  cosineSimilarity,
  topKByScore,
  openAiEmbeddingsProvider,
  openRouterEmbeddingsProvider,
  BUILT_IN_EMBEDDING_PROVIDERS,
} from './embeddings.js';
export type { EmbeddingProvider, EmbeddingResult } from './embeddings.js';

export { errors } from './errors.js';
