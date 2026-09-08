import { table, t } from 'spacetimedb/server';

export const apiKey = table(
  { name: 'api_key', public: false },
  {
    provider: t.string().primaryKey(),
    key: t.string(),
    updatedAt: t.timestamp(),
  }
);

export const agentSecret = table(
  { name: 'agent_secret', public: false },
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
    addedAtMicros: t.i64(),
  }
);

// Effective config precedence: thread > override > code default.
export const agentOverride = table(
  { name: 'agent_override', public: true },
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

export const thread = table(
  { name: 'thread', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity().index(),
    agentName: t.string().index(),
    title: t.option(t.string()),
    systemPromptOverride: t.option(t.string()),
    modelOverride: t.option(t.string()),
    metadata: t.option(t.string()),
    summary: t.option(t.string()),
    summarizedThroughId: t.option(t.u64()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

// owner denormalized from thread so the visibility view can filter on it.
export const message = table(
  { name: 'message', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    threadId: t.u64().index(),
    owner: t.identity().index(),
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
    owner: t.identity().index(),
    lockedAt: t.timestamp().index('btree'),
    cancelRequested: t.bool(),
  }
);

export const messageEmbedding = table(
  { name: 'message_embedding', public: false },
  {
    messageId: t.u64().primaryKey(),
    threadId: t.u64().index(),
    owner: t.identity().index(),
    model: t.string(),
    vector: t.array(t.f32()),
    createdAt: t.timestamp(),
  }
);
