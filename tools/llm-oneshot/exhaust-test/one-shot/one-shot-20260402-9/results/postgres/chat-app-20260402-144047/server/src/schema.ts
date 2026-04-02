import { pgTable, text, timestamp, boolean, integer, primaryKey, pgEnum } from 'drizzle-orm/pg-core';

export const userStatusEnum = pgEnum('user_status', ['online', 'away', 'dnd', 'invisible']);

export const users = pgTable('users', {
  id: text('id').primaryKey(),
  username: text('username').notNull().unique(),
  status: userStatusEnum('status').notNull().default('online'),
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  socketId: text('socket_id'),
});

export const rooms = pgTable('rooms', {
  id: text('id').primaryKey(),
  name: text('name').notNull().unique(),
  creatorId: text('creator_id').notNull().references(() => users.id),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable('room_members', {
  roomId: text('room_id').notNull().references(() => rooms.id),
  userId: text('user_id').notNull().references(() => users.id),
  role: text('role').notNull().default('member'),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
  isBanned: boolean('is_banned').notNull().default(false),
}, (t) => ({
  pk: primaryKey({ columns: [t.roomId, t.userId] }),
}));

export const messages = pgTable('messages', {
  id: text('id').primaryKey(),
  roomId: text('room_id').notNull().references(() => rooms.id),
  senderId: text('sender_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  editedAt: timestamp('edited_at'),
  isEphemeral: boolean('is_ephemeral').notNull().default(false),
  expiresAt: timestamp('expires_at'),
});

export const messageHistory = pgTable('message_history', {
  id: text('id').primaryKey(),
  messageId: text('message_id').notNull().references(() => messages.id),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
});

export const readReceipts = pgTable('read_receipts', {
  messageId: text('message_id').notNull().references(() => messages.id),
  userId: text('user_id').notNull().references(() => users.id),
  readAt: timestamp('read_at').notNull().defaultNow(),
}, (t) => ({
  pk: primaryKey({ columns: [t.messageId, t.userId] }),
}));

export const messageReactions = pgTable('message_reactions', {
  id: text('id').primaryKey(),
  messageId: text('message_id').notNull().references(() => messages.id),
  userId: text('user_id').notNull().references(() => users.id),
  emoji: text('emoji').notNull(),
  reactedAt: timestamp('reacted_at').notNull().defaultNow(),
});

export const scheduledMessages = pgTable('scheduled_messages', {
  id: text('id').primaryKey(),
  roomId: text('room_id').notNull().references(() => rooms.id),
  senderId: text('sender_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  scheduledAt: timestamp('scheduled_at').notNull(),
  isSent: boolean('is_sent').notNull().default(false),
  cancelledAt: timestamp('cancelled_at'),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});
