import { table, t } from 'spacetimedb/server';

export const apiKey = table(
  { name: 'api_key', public: false },
  {
    provider: t.string().primaryKey(),
    key: t.string(),
    updatedAt: t.timestamp(),
  }
);

export const agentConfig = table(
  { name: 'agent_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    staleLockThresholdSecs: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const agentAdminIdentity = table(
  { name: 'agent_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAt: t.timestamp(),
  }
);

// Operator overrides take precedence over agent code defaults.
export const agentOverride = table(
  { name: 'agent_override', public: false },
  {
    agentName: t.string().primaryKey(),
    provider: t.option(t.string()),
    model: t.option(t.string()),
    systemPrompt: t.option(t.string()),
    maxTurns: t.option(t.u32()),
    maxHistoryMessages: t.option(t.u32()),
    maxTokens: t.option(t.u32()),
    retries: t.option(t.u32()),
    updatedAt: t.timestamp(),
  }
);

// owner is the application-defined key passed to the client, such as a user id.
export const thread = table(
  { name: 'thread', public: false },
  {
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
  }
);

// owner is denormalized from thread so host views can filter on it.
export const message = table(
  { name: 'message', public: false },
  {
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
  }
);

// Presence of a row is the per-thread mutex: a loop is running for it.
export const threadLock = table(
  { name: 'thread_lock', public: false },
  {
    threadId: t.u64().primaryKey(),
    owner: t.string().index(),
    lockedAt: t.timestamp().index('btree'),
    cancelRequested: t.bool(),
  }
);

export const threadLockSweeperTick = table(
  { name: 'thread_lock_sweeper_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export const messageEmbedding = table(
  { name: 'message_embedding', public: false },
  {
    messageId: t.u64().primaryKey(),
    threadId: t.u64().index(),
    owner: t.string().index(),
    model: t.string(),
    vector: t.array(t.f32()),
    createdAt: t.timestamp(),
  }
);
