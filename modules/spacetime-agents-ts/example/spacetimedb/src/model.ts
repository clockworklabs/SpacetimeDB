import { table, t } from 'spacetimedb/server';

// Per-user token cap; no row means no cap.
export const tokenLimit = table(
  { name: 'token_limit', public: false },
  {
    singleton: t.bool().primaryKey(),
    tokensPerWindow: t.u32(),
    windowSecs: t.u32(),
    updatedAt: t.timestamp(),
  }
);

// One row per user: tokens spent in the user's current window.
export const tokenUsage = table(
  { name: 'token_usage', public: false },
  {
    owner: t.string().primaryKey(),
    tokens: t.u64(),
    windowEndsAt: t.timestamp(),
  }
);

export const messageAttachment = table(
  { name: 'message_attachment', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    fileId: t.u64().index(),
    messageId: t.u64().index(),
    threadId: t.u64().index(),
    ownerUserId: t.string().index(),
    ordinal: t.u32(),
    filename: t.option(t.string()),
    createdAt: t.timestamp(),
  }
);

export const fileViewRow = t.object('File', {
  id: t.u64(),
  fileId: t.u64(),
  path: t.string(),
  ownerUserId: t.string(),
  mimeType: t.string(),
  size: t.u64(),
  sha256Hex: t.string(),
  visibility: t.string(),
  filename: t.option(t.string()),
  messageId: t.option(t.u64()),
  threadId: t.option(t.u64()),
  ordinal: t.u32(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
});
