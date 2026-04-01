import { schema, table, t } from 'spacetimedb/server';

// Users table - tracks all connected users
export const User = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    lastSeen: t.timestamp(),
  }
);

// Rooms table
export const Room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Room membership table
export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { accessor: 'room_member_room_id', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'room_member_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    joinedAt: t.timestamp(),
  }
);

// Messages table
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { accessor: 'message_room_id', algorithm: 'btree', columns: ['roomId'] },
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

// Typing indicator table
export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { accessor: 'typing_room_id', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    updatedAt: t.timestamp(),
  }
);

// Read receipt table - tracks last-read message per user per room
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { accessor: 'read_receipt_room_id', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'read_receipt_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    lastReadMessageId: t.u64(),
    readAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  user: User,
  room: Room,
  room_member: RoomMember,
  message: Message,
  typing_indicator: TypingIndicator,
  read_receipt: ReadReceipt,
});

export default spacetimedb;
