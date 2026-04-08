import { schema, table, t } from 'spacetimedb/server';

// User table - stores user information and online status
export const user = table(
  {
    name: 'user',
    public: true,
    indexes: [
      { name: 'identity', algorithm: 'btree', columns: ['identity'] },
      { name: 'name', algorithm: 'btree', columns: ['name'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    identity: t.identity(),
    name: t.string(),
    online: t.bool(),
    lastSeen: t.timestamp(),
  }
);

// Room table - stores chat rooms
export const Room = table(
  {
    name: 'room',
    public: true,
    indexes: [{ name: 'name', algorithm: 'btree', columns: ['name'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    description: t.string().optional(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// RoomMember table - stores room membership and roles
export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      {
        name: 'room_identity',
        algorithm: 'btree',
        columns: ['roomId', 'identity'],
      },
      { name: 'room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    role: t.string(), // 'owner', 'admin', 'member'
    joinedAt: t.timestamp(),
  }
);

// Message table - stores chat messages
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'room_id', algorithm: 'btree', columns: ['roomId'] },
      {
        name: 'sender_timestamp',
        algorithm: 'btree',
        columns: ['senderId', 'sentAt'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderId: t.identity(),
    senderName: t.string(),
    content: t.string(),
    sentAt: t.timestamp(),
    editedAt: t.timestamp().optional(),
    isEphemeral: t.bool(),
    ephemeralExpiresAt: t.timestamp().optional(),
  }
);

// MessageEdit table - stores edit history for messages
export const MessageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [
      { name: 'message_id', algorithm: 'btree', columns: ['messageId'] },
      {
        name: 'message_timestamp',
        algorithm: 'btree',
        columns: ['messageId', 'editedAt'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    previousContent: t.string(),
    newContent: t.string(),
    editedAt: t.timestamp(),
    editedBy: t.identity(),
  }
);

// Reaction table - stores emoji reactions to messages
export const Reaction = table(
  {
    name: 'reaction',
    public: true,
    indexes: [
      { name: 'message_id', algorithm: 'btree', columns: ['messageId'] },
      {
        name: 'message_user',
        algorithm: 'btree',
        columns: ['messageId', 'userId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userId: t.identity(),
    emoji: t.string(),
    reactedAt: t.timestamp(),
  }
);

// ReadReceipt table - stores who has read which messages
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'message_id', algorithm: 'btree', columns: ['messageId'] },
      { name: 'user_id', algorithm: 'btree', columns: ['userId'] },
      {
        name: 'user_message',
        algorithm: 'btree',
        columns: ['userId', 'messageId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userId: t.identity(),
    readAt: t.timestamp(),
  }
);

// RoomReadPosition table - stores last read position per user per room
export const RoomReadPosition = table(
  {
    name: 'room_read_position',
    public: true,
    indexes: [
      { name: 'room_user', algorithm: 'btree', columns: ['roomId', 'userId'] },
      { name: 'user_id', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    lastReadMessageId: t.u64(),
    lastReadAt: t.timestamp(),
  }
);

// TypingIndicator table - stores current typing status
export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { name: 'room_user', algorithm: 'btree', columns: ['roomId', 'userId'] },
      { name: 'room_id', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    userName: t.string(),
    startedTypingAt: t.timestamp(),
  }
);

// ScheduledMessage table - stores messages scheduled to send later
export const ScheduledMessage = table(
  {
    name: 'scheduled_message',
    scheduled: 'send_scheduled_message',
    indexes: [
      {
        name: 'room_sender',
        algorithm: 'btree',
        columns: ['roomId', 'senderId'],
      },
    ],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    senderId: t.identity(),
    senderName: t.string(),
    content: t.string(),
  }
);

// EphemeralMessage table - stores disappearing messages (auto-cleanup)
export const EphemeralMessage = table(
  {
    name: 'ephemeral_message',
    scheduled: 'cleanup_ephemeral_message',
    indexes: [
      { name: 'message_id', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  Message,
  MessageEdit,
  Reaction,
  ReadReceipt,
  RoomReadPosition,
  TypingIndicator,
  ScheduledMessage,
  EphemeralMessage
);
