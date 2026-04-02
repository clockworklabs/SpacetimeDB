import { pgTable, serial, text, integer, boolean, timestamp, primaryKey, unique } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  username: text('username').notNull().unique(),
  status: text('status').notNull().default('online'), // online, away, dnd, invisible
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdBy: integer('created_by').notNull().references(() => users.id),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable('room_members', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  isAdmin: boolean('is_admin').notNull().default(false),
  isBanned: boolean('is_banned').notNull().default(false),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
}, (t) => [unique().on(t.roomId, t.userId)]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  isEdited: boolean('is_edited').notNull().default(false),
  isEphemeral: boolean('is_ephemeral').notNull().default(false),
  ephemeralExpiresAt: timestamp('ephemeral_expires_at'),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  updatedAt: timestamp('updated_at').notNull().defaultNow(),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  oldContent: text('old_content').notNull(),
  newContent: text('new_content').notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
});

export const readReceipts = pgTable('read_receipts', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  seenAt: timestamp('seen_at').notNull().defaultNow(),
}, (t) => [unique().on(t.messageId, t.userId)]);

export const lastRead = pgTable('last_read', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  lastMessageId: integer('last_message_id'),
  updatedAt: timestamp('updated_at').notNull().defaultNow(),
}, (t) => [unique().on(t.roomId, t.userId)]);

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  scheduledFor: timestamp('scheduled_for').notNull(),
  isSent: boolean('is_sent').notNull().default(false),
  isCancelled: boolean('is_cancelled').notNull().default(false),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const reactions = pgTable('reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
}, (t) => [unique().on(t.messageId, t.userId, t.emoji)]);
