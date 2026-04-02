import { table, t } from 'spacetimedb/server';

// User profile and presence
export const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    status: t.string(), // online, away, dnd, invisible
    lastActive: t.timestamp(),
  }
);

// Chat rooms
export const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Room membership and roles
export const roomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree' as const, columns: ['roomId'] as const },
      { name: 'by_identity', algorithm: 'btree' as const, columns: ['identity'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    role: t.string(), // member, admin
    banned: t.bool(),
  }
);

// Messages
export const message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree' as const, columns: ['roomId'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    editedAt: t.option(t.timestamp()),
    isEphemeral: t.bool(),
    expiresAt: t.option(t.timestamp()),
    deleted: t.bool(),
  }
);

// Message edit history
export const messageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree' as const, columns: ['messageId'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    oldText: t.string(),
    editedAt: t.timestamp(),
  }
);

// Typing indicators
export const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree' as const, columns: ['roomId'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    lastTypedAt: t.timestamp(),
  }
);

// Read receipts
export const readReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree' as const, columns: ['messageId'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    reader: t.identity(),
    seenAt: t.timestamp(),
  }
);

// Last read position per user per room (for unread counts)
export const roomLastRead = table(
  {
    name: 'room_last_read',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree' as const, columns: ['roomId'] as const },
      { name: 'by_identity', algorithm: 'btree' as const, columns: ['identity'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    lastReadMessageId: t.u64(),
  }
);

// Message reactions
export const messageReaction = table(
  {
    name: 'message_reaction',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree' as const, columns: ['messageId'] as const },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    reactor: t.identity(),
    emoji: t.string(),
  }
);
