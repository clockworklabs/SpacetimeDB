import { table, t } from 'spacetimedb/server';

const apiKeyOptions = { name: 'api_key', public: false };
const apiKeyColumns = {
  provider: t.string().primaryKey(),
  key: t.string(),
  updatedAt: t.timestamp(),
};
export const apiKey: ReturnType<
  typeof table<typeof apiKeyColumns, typeof apiKeyOptions>
> = table(apiKeyOptions, apiKeyColumns);

const agentConfigOptions = { name: 'agent_config', public: false };
const agentConfigColumns = {
  singleton: t.bool().primaryKey(),
  staleLockThresholdSecs: t.u32(),
  updatedAt: t.timestamp(),
};
export const agentConfig: ReturnType<
  typeof table<typeof agentConfigColumns, typeof agentConfigOptions>
> = table(agentConfigOptions, agentConfigColumns);

const agentAdminIdentityOptions = {
  name: 'agent_admin_identity',
  public: false,
};
const agentAdminIdentityColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
export const agentAdminIdentity: ReturnType<
  typeof table<
    typeof agentAdminIdentityColumns,
    typeof agentAdminIdentityOptions
  >
> = table(agentAdminIdentityOptions, agentAdminIdentityColumns);

// Operator overrides take precedence over agent code defaults.
const agentOverrideOptions = { name: 'agent_override', public: false };
const agentOverrideColumns = {
  agentName: t.string().primaryKey(),
  provider: t.option(t.string()),
  model: t.option(t.string()),
  systemPrompt: t.option(t.string()),
  maxTurns: t.option(t.u32()),
  maxHistoryMessages: t.option(t.u32()),
  maxTokens: t.option(t.u32()),
  retries: t.option(t.u32()),
  updatedAt: t.timestamp(),
};
export const agentOverride: ReturnType<
  typeof table<typeof agentOverrideColumns, typeof agentOverrideOptions>
> = table(agentOverrideOptions, agentOverrideColumns);

// owner is the application-defined key passed to the client, such as a user id.
const threadOptions = { name: 'thread', public: false };
const threadColumns = {
  id: t.u64().primaryKey().autoInc(),
  owner: t.string().index(),
  agentName: t.string().index(),
  title: t.option(t.string()),
  modelOverride: t.option(t.string()),
  metadata: t.option(t.string()),
  summary: t.option(t.string()),
  summarizedThroughId: t.option(t.u64()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};
export const thread: ReturnType<
  typeof table<typeof threadColumns, typeof threadOptions>
> = table(threadOptions, threadColumns);

// owner is denormalized from thread so host views can filter on it.
const messageOptions = { name: 'message', public: false };
const messageColumns = {
  id: t.u64().primaryKey().autoInc(),
  threadId: t.u64().index(),
  owner: t.string().index(),
  role: t.string(),
  content: t.string(),
  toolCallsJson: t.option(t.string()),
  toolCallId: t.option(t.string()),
  isError: t.bool(),
  promptTokens: t.option(t.u32()),
  completionTokens: t.option(t.u32()),
  createdAt: t.timestamp(),
};
export const message: ReturnType<
  typeof table<typeof messageColumns, typeof messageOptions>
> = table(messageOptions, messageColumns);

// Presence of a row is the per-thread mutex: a loop is running for it.
const threadLockOptions = { name: 'thread_lock', public: false };
const threadLockColumns = {
  threadId: t.u64().primaryKey(),
  owner: t.string().index(),
  lockedAt: t.timestamp().index('btree'),
  cancelRequested: t.bool(),
  runId: t.string().default(''),
};
export const threadLock: ReturnType<
  typeof table<typeof threadLockColumns, typeof threadLockOptions>
> = table(threadLockOptions, threadLockColumns);

const threadLockSweeperTickOptions = { name: 'thread_lock_sweeper_tick' };
const threadLockSweeperTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const threadLockSweeperTick: ReturnType<
  typeof table<
    typeof threadLockSweeperTickColumns,
    typeof threadLockSweeperTickOptions
  >
> = table(threadLockSweeperTickOptions, threadLockSweeperTickColumns);

const messageEmbeddingOptions = { name: 'message_embedding', public: false };
const messageEmbeddingColumns = {
  messageId: t.u64().primaryKey(),
  threadId: t.u64().index(),
  owner: t.string().index(),
  model: t.string(),
  vector: t.array(t.f32()),
  createdAt: t.timestamp(),
};
export const messageEmbedding: ReturnType<
  typeof table<typeof messageEmbeddingColumns, typeof messageEmbeddingOptions>
> = table(messageEmbeddingOptions, messageEmbeddingColumns);
