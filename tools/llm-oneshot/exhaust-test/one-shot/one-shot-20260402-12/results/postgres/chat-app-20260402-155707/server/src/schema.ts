import { pgTable, serial, text, timestamp, boolean, integer, primaryKey } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  status: text('status').notNull().default('online'),
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdBy: integer('created_by').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable('room_members', {
  roomId: integer('room_id').notNull(),
  userId: integer('user_id').notNull(),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
}, (table) => ({
  pk: primaryKey({ columns: [table.roomId, table.userId] }),
}));

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull(),
  userId: integer('user_id').notNull(),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  expiresAt: timestamp('expires_at'),
  scheduledFor: timestamp('scheduled_for'),
  isSent: boolean('is_sent').notNull().default(true),
  isDeleted: boolean('is_deleted').notNull().default(false),
});

export const messageReadReceipts = pgTable('message_read_receipts', {
  messageId: integer('message_id').notNull(),
  userId: integer('user_id').notNull(),
  seenAt: timestamp('seen_at').notNull().defaultNow(),
}, (table) => ({
  pk: primaryKey({ columns: [table.messageId, table.userId] }),
}));

export const messageReactions = pgTable('message_reactions', {
  messageId: integer('message_id').notNull(),
  userId: integer('user_id').notNull(),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
}, (table) => ({
  pk: primaryKey({ columns: [table.messageId, table.userId, table.emoji] }),
}));
