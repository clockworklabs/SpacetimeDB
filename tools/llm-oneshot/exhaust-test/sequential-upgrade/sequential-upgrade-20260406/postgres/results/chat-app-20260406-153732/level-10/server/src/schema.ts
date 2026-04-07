import { pgTable, serial, text, timestamp, integer, boolean, primaryKey, type AnyPgColumn } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  online: boolean('online').default(false).notNull(),
  status: text('status').default('offline').notNull(), // 'online' | 'away' | 'dnd' | 'invisible' | 'offline'
  lastSeen: timestamp('last_seen').defaultNow().notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  isPrivate: boolean('is_private').default(false).notNull(),
  isDm: boolean('is_dm').default(false).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const roomMembers = pgTable('room_members', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  isAdmin: boolean('is_admin').default(false).notNull(),
  joinedAt: timestamp('joined_at').defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const bannedUsers = pgTable('banned_users', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  bannedAt: timestamp('banned_at').defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  expiresAt: timestamp('expires_at'),
  editedAt: timestamp('edited_at'),
  parentMessageId: integer('parent_message_id').references((): AnyPgColumn => messages.id, { onDelete: 'cascade' }),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const readReceipts = pgTable('read_receipts', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  seenAt: timestamp('seen_at').defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.messageId] }),
]);

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  scheduledFor: timestamp('scheduled_for').notNull(),
  sent: boolean('sent').default(false).notNull(),
  cancelled: boolean('cancelled').default(false).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const messageReactions = pgTable('message_reactions', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.messageId, t.emoji] }),
]);

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
});

export const roomInvitations = pgTable('room_invitations', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  inviterId: integer('inviter_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  inviteeId: integer('invitee_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  status: text('status').default('pending').notNull(), // 'pending' | 'accepted' | 'declined'
  createdAt: timestamp('created_at').defaultNow().notNull(),
});
