import { schema, table, t } from 'spacetimedb/server';

// Users — one row per connected identity
export const User = table(
  {
    name: 'user',
    public: true,
  },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    createdAt: t.timestamp(),
  }
);

// Chat rooms
export const Room = table(
  {
    name: 'room',
    public: true,
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Room membership
export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { name: 'room_member_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'room_member_identity', algorithm: 'btree', columns: ['memberIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    memberIdentity: t.identity(),
    joinedAt: t.timestamp(),
  }
);

// Chat messages
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'message_room_id', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderId: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
  }
);

// Typing indicators — each row = one user typing in one room
// Clients filter by expiresAt < now to handle expiry
export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { name: 'typing_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'typing_user_identity', algorithm: 'btree', columns: ['userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
    expiresAt: t.timestamp(),
  }
);

// Read receipts — which user saw which message
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'read_receipt_message_id', algorithm: 'btree', columns: ['messageId'] },
      { name: 'read_receipt_user_identity', algorithm: 'btree', columns: ['userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    seenAt: t.timestamp(),
  }
);

// Last-read position per user per room (for unread counts)
export const UserRoomRead = table(
  {
    name: 'user_room_read',
    public: true,
    indexes: [
      { name: 'user_room_read_identity', algorithm: 'btree', columns: ['userIdentity'] },
      { name: 'user_room_read_room_id', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    userIdentity: t.identity(),
    roomId: t.u64(),
    lastReadMessageId: t.u64(),
    lastReadAt: t.timestamp(),
  }
);

// Spread form — the only form that works with the current SDK
export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  Message,
  TypingIndicator,
  ReadReceipt,
  UserRoomRead
);
export default spacetimedb;
