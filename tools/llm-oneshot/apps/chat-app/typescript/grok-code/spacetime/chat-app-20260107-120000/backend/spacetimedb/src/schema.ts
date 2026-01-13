import { schema, table, t } from 'spacetimedb/server';

// Users table - stores user information and display names
export const User = table({
  name: 'user',
  public: true
}, {
  identity: t.identity().primaryKey(),
  displayName: t.string(),
  createdAt: t.timestamp(),
  lastSeen: t.timestamp(),
  isOnline: t.bool(),
});

// Rooms table - chat rooms that users can join
export const Room = table({
  name: 'room',
  public: true,
  indexes: [{ name: 'room_owner_id', algorithm: 'btree', columns: ['ownerId'] }]
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  description: t.string().optional(),
  ownerId: t.identity(),
  createdAt: t.timestamp(),
  isPublic: t.bool(),
});

// Room members - tracks which users are in which rooms
export const RoomMember = table({
  name: 'room_member',
  public: true,
  indexes: [
    { name: 'room_member_room_id', algorithm: 'btree', columns: ['roomId'] },
    { name: 'room_member_user_id', algorithm: 'btree', columns: ['userId'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  userId: t.identity(),
  joinedAt: t.timestamp(),
  lastReadMessageId: t.u64().optional(), // For unread count tracking
});

// Messages table - main chat messages
export const Message = table({
  name: 'message',
  public: true,
  indexes: [
    { name: 'message_room_id', algorithm: 'btree', columns: ['roomId'] },
    { name: 'message_author_id', algorithm: 'btree', columns: ['authorId'] },
    { name: 'message_created_at', algorithm: 'btree', columns: ['createdAt'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  authorId: t.identity(),
  content: t.string(),
  createdAt: t.timestamp(),
  editedAt: t.timestamp().optional(),
  isEdited: t.bool(),
});

// Message edits table - tracks edit history for messages
export const MessageEdit = table({
  name: 'message_edit',
  public: true,
  indexes: [
    { name: 'message_edit_message_id', algorithm: 'btree', columns: ['messageId'] },
    { name: 'message_edit_edited_at', algorithm: 'btree', columns: ['editedAt'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  messageId: t.u64(),
  previousContent: t.string(),
  newContent: t.string(),
  editedAt: t.timestamp(),
  editedBy: t.identity(),
});

// Scheduled messages - messages that will be sent at a future time
export const ScheduledMessage = table({
  name: 'scheduled_message',
  scheduled: 'send_scheduled_message'
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  roomId: t.u64(),
  authorId: t.identity(),
  content: t.string(),
  createdAt: t.timestamp(),
});

// Ephemeral messages - messages that auto-delete after duration
export const EphemeralMessage = table({
  name: 'ephemeral_message',
  scheduled: 'delete_ephemeral_message'
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  messageId: t.u64(),
  roomId: t.u64(),
  authorId: t.identity(),
  content: t.string(),
  createdAt: t.timestamp(),
  durationMinutes: t.u64(), // How long before deletion
});

// Typing indicators - shows when users are typing
export const TypingIndicator = table({
  name: 'typing_indicator',
  public: true,
  indexes: [
    { name: 'typing_indicator_room_id', algorithm: 'btree', columns: ['roomId'] },
    { name: 'typing_indicator_user_id', algorithm: 'btree', columns: ['userId'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  userId: t.identity(),
  startedAt: t.timestamp(),
});

// Read receipts - tracks who has seen which messages
export const ReadReceipt = table({
  name: 'read_receipt',
  public: true,
  indexes: [
    { name: 'read_receipt_message_id', algorithm: 'btree', columns: ['messageId'] },
    { name: 'read_receipt_user_id', algorithm: 'btree', columns: ['userId'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  messageId: t.u64(),
  userId: t.identity(),
  readAt: t.timestamp(),
});

// Message reactions - emoji reactions to messages
export const MessageReaction = table({
  name: 'message_reaction',
  public: true,
  indexes: [
    { name: 'message_reaction_message_id', algorithm: 'btree', columns: ['messageId'] },
    { name: 'message_reaction_user_id', algorithm: 'btree', columns: ['userId'] },
    { name: 'message_reaction_emoji', algorithm: 'btree', columns: ['emoji'] }
  ]
}, {
  id: t.u64().primaryKey().autoInc(),
  messageId: t.u64(),
  userId: t.identity(),
  emoji: t.string(),
  reactedAt: t.timestamp(),
});

// User status - tracks online/offline status
export const UserStatus = table({
  name: 'user_status',
  public: true
}, {
  identity: t.identity().primaryKey(),
  isOnline: t.bool(),
  lastSeen: t.timestamp(),
});

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  Message,
  MessageEdit,
  ScheduledMessage,
  EphemeralMessage,
  TypingIndicator,
  ReadReceipt,
  MessageReaction,
  UserStatus
);