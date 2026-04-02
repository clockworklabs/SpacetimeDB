import { table, t } from 'spacetimedb/server';

// ==================== USER TABLES ====================

export const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(), // 'online', 'away', 'dnd', 'invisible'
    lastActive: t.timestamp(),
  }
);

// ==================== ROOM TABLES ====================

export const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

export const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    role: t.string(), // 'admin', 'member'
    joinedAt: t.timestamp(),
  }
);

export const roomBan = table(
  { name: 'room_ban', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity(),
    bannedAt: t.timestamp(),
  }
);

// ==================== MESSAGE TABLES ====================

export const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    isEdited: t.bool(),
    isEphemeral: t.bool(),
    expiresAt: t.option(t.timestamp()),
  }
);

export const messageHistory = table(
  { name: 'message_history', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    oldText: t.string(),
    editedAt: t.timestamp(),
  }
);

export const messageReaction = table(
  { name: 'message_reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

// ==================== PRESENCE & RECEIPTS ====================

export const typingStatus = table(
  { name: 'typing_status', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    lastTypedAt: t.timestamp(),
  }
);

export const readReceipt = table(
  { name: 'read_receipt', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userIdentity: t.identity(),
    readAt: t.timestamp(),
  }
);

export const roomLastRead = table(
  { name: 'room_last_read', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity().index('btree'),
    lastReadMessageId: t.u64(),
  }
);

// Table definitions only — schema is created in index.ts (which includes scheduled tables)
