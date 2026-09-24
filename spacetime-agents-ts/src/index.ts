export {
  agentTool,
  makeAgentDispatch,
  defineAgent,
  makeAgentRegistry,
  typeBuilderToJsonSchema,
} from './agent';
export type {
  AgentTool,
  AgentDefinition,
  AgentRegistry,
  InvokeResult,
  ToolMap,
} from './agent';

export { callChat, isRetryableError } from './openrouter';
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
} from './openrouter';

export {
  openRouterProvider,
  openAiProvider,
  anthropicProvider,
  BUILT_IN_PROVIDERS,
} from './providers';

export {
  cosineSimilarity,
  topKByScore,
  openAiEmbeddingsProvider,
  openRouterEmbeddingsProvider,
  BUILT_IN_EMBEDDING_PROVIDERS,
} from './embeddings';
export type { EmbeddingProvider, EmbeddingResult } from './embeddings';

export {
  pickSummarizationCandidates,
  formatMessagesForSummarizer,
  buildSummarizerUserContent,
  augmentSystemWithSummary,
} from './submodule/summarize';
