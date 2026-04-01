import { schema, table, t } from 'spacetimedb/server';

// One row per connected identity
export const User = table({
  name: 'user',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  username: t.string(),
  isOnline: t.bool(),
});

// Chat rooms
export const Room = table({
  name: 'room',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  createdBy: t.identity(),
  createdAt: t.timestamp(),
});

// Room membership
export const RoomMember = table({
  name: 'room_member',
  public: true,
  indexes: [
    { name: 'room_member_room_id', algorithm: 'btree', columns: ['roomId'] },
    { name: 'room_member_user_id', algorithm: 'btree', columns: ['userId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  userId: t.identity(),
});

// Chat messages
export const Message = table({
  name: 'message',
  public: true,
  indexes: [
    { name: 'message_room_id', algorithm: 'btree', columns: ['roomId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  sender: t.identity(),
  content: t.string(),
  sentAt: t.timestamp(),
});

// Typing indicators — userId is PK, one entry per user (cross-room)
export const TypingIndicator = table({
  name: 'typing_indicator',
  public: true,
  indexes: [
    { name: 'typing_room_id', algorithm: 'btree', columns: ['roomId'] },
  ],
}, {
  userId: t.identity().primaryKey(),
  roomId: t.u64(),
});

// Last-read position per user per room (for unread counts + read receipts)
export const UserRoomRead = table({
  name: 'user_room_read',
  public: true,
  indexes: [
    { name: 'user_room_read_user_id', algorithm: 'btree', columns: ['userId'] },
    { name: 'user_room_read_room_id', algorithm: 'btree', columns: ['roomId'] },
  ],
}, {
  id: t.u64().primaryKey().autoInc(),
  userId: t.identity(),
  roomId: t.u64(),
  lastMessageId: t.u64(),
});

const spacetimedb = schema(User, Room, RoomMember, Message, TypingIndicator, UserRoomRead);
export default spacetimedb;
