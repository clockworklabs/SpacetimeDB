import { pgTable, serial, text, timestamp, integer, primaryKey, boolean } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdAt: timestamp('created_at', { withTimezone: true }).defaultNow().notNull(),
  socketId: text('socket_id'),
  status: text('status').notNull().default('online'), // 'online' | 'away' | 'do-not-disturb' | 'invisible'
  lastActiveAt: timestamp('last_active_at', { withTimezone: true }).defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdBy: integer('created_by').notNull().references(() => users.id),
  createdAt: timestamp('created_at', { withTimezone: true }).defaultNow().notNull(),
});

export const roomMembers = pgTable('room_members', {
  userId: integer('user_id').notNull().references(() => users.id),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  joinedAt: timestamp('joined_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  createdAt: timestamp('created_at', { withTimezone: true }).defaultNow().notNull(),
  expiresAt: timestamp('expires_at', { withTimezone: true }),
  editedAt: timestamp('edited_at', { withTimezone: true }),
  isEdited: boolean('is_edited').default(false).notNull(),
  parentMessageId: integer('parent_message_id'), // null = root message, non-null = thread reply
});

export const messageReads = pgTable('message_reads', {
  messageId: integer('message_id').notNull().references(() => messages.id),
  userId: integer('user_id').notNull().references(() => users.id),
  readAt: timestamp('read_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.messageId, t.userId] }),
]);

export const lastReadPositions = pgTable('last_read_positions', {
  userId: integer('user_id').notNull().references(() => users.id),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  lastMessageId: integer('last_message_id'),
  updatedAt: timestamp('updated_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  scheduledFor: timestamp('scheduled_for', { withTimezone: true }).notNull(),
  status: text('status').notNull().default('pending'), // 'pending' | 'sent' | 'cancelled'
  createdAt: timestamp('created_at', { withTimezone: true }).defaultNow().notNull(),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id),
  userId: integer('user_id').notNull().references(() => users.id),
  previousContent: text('previous_content').notNull(),
  editedAt: timestamp('edited_at', { withTimezone: true }).defaultNow().notNull(),
});

export const messageReactions = pgTable('message_reactions', {
  messageId: integer('message_id').notNull().references(() => messages.id),
  userId: integer('user_id').notNull().references(() => users.id),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.messageId, t.userId, t.emoji] }),
]);

export const roomAdmins = pgTable('room_admins', {
  userId: integer('user_id').notNull().references(() => users.id),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  grantedAt: timestamp('granted_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const roomBans = pgTable('room_bans', {
  userId: integer('user_id').notNull().references(() => users.id),
  roomId: integer('room_id').notNull().references(() => rooms.id),
  bannedBy: integer('banned_by').notNull().references(() => users.id),
  bannedAt: timestamp('banned_at', { withTimezone: true }).defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);
