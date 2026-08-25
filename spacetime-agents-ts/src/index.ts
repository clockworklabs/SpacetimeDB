export {
  agentTool,
  makeAgentDispatch,
  defineAgent,
  makeAgentRegistry,
  typeBuilderToJsonSchema,
} from './kit.ts';
export type {
  AgentTool,
  AgentDefinition,
  AgentRegistry,
  InvokeResult,
  ToolMap,
} from './kit.ts';

export { callChat, isRetryableError } from './openrouter.ts';
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
  ParsedResponse,
} from './openrouter.ts';

export {
  openRouterProvider,
  openAiProvider,
  anthropicProvider,
  BUILT_IN_PROVIDERS,
} from './providers.ts';

export {
  cosineSimilarity,
  topKByScore,
  openAiEmbeddingsProvider,
  openRouterEmbeddingsProvider,
  BUILT_IN_EMBEDDING_PROVIDERS,
} from './embeddings.ts';
export type { EmbeddingProvider, EmbeddingResult } from './embeddings.ts';
