import { pgTable, text, timestamp, integer, boolean, pgEnum } from 'drizzle-orm/pg-core';

export const statusEnum = pgEnum('user_status', ['online', 'away', 'dnd', 'invisible']);

export const users = pgTable('users', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  status: statusEnum('status').notNull().default('online'),
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
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
  isAdmin: boolean('is_admin').notNull().default(false),
  isBanned: boolean('is_banned').notNull().default(false),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
});

export const messages = pgTable('messages', {
  id: text('id').primaryKey(),
  roomId: text('room_id').notNull().references(() => rooms.id),
  userId: text('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  isEdited: boolean('is_edited').notNull().default(false),
  expiresAt: timestamp('expires_at'),
  scheduledAt: timestamp('scheduled_at'),
  isSent: boolean('is_sent').notNull().default(true),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const messageEdits = pgTable('message_edits', {
  id: text('id').primaryKey(),
  messageId: text('message_id').notNull().references(() => messages.id),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
});

export const readReceipts = pgTable('read_receipts', {
  messageId: text('message_id').notNull().references(() => messages.id),
  userId: text('user_id').notNull().references(() => users.id),
  readAt: timestamp('read_at').notNull().defaultNow(),
});

export const lastRead = pgTable('last_read', {
  roomId: text('room_id').notNull().references(() => rooms.id),
  userId: text('user_id').notNull().references(() => users.id),
  lastMessageId: text('last_message_id'),
  updatedAt: timestamp('updated_at').notNull().defaultNow(),
});

export const reactions = pgTable('reactions', {
  id: text('id').primaryKey(),
  messageId: text('message_id').notNull().references(() => messages.id),
  userId: text('user_id').notNull().references(() => users.id),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const scheduledMessages = pgTable('scheduled_messages', {
  id: text('id').primaryKey(),
  roomId: text('room_id').notNull().references(() => rooms.id),
  userId: text('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  scheduledAt: timestamp('scheduled_at').notNull(),
  isCancelled: boolean('is_cancelled').notNull().default(false),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});
