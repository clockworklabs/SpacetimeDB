import { pgTable, serial, text, timestamp, integer, primaryKey, unique } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  username: text('username').notNull().unique(),
  status: text('status').notNull().default('online'),
  lastActiveAt: timestamp('last_active_at').defaultNow().notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  creatorId: integer('creator_id').references(() => users.id, { onDelete: 'set null' }),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const roomMembers = pgTable('room_members', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  role: text('role').notNull().default('member'),
  joinedAt: timestamp('joined_at').defaultNow().notNull(),
}, (t) => [primaryKey({ columns: [t.userId, t.roomId] })]);

export const roomBans = pgTable('room_bans', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  bannedBy: integer('banned_by').references(() => users.id, { onDelete: 'set null' }),
  bannedAt: timestamp('banned_at').defaultNow().notNull(),
}, (t) => [primaryKey({ columns: [t.userId, t.roomId] })]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  expiresAt: timestamp('expires_at'),
  editedAt: timestamp('edited_at'),
});

export const messageReads = pgTable('message_reads', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  readAt: timestamp('read_at').defaultNow().notNull(),
}, (t) => [primaryKey({ columns: [t.userId, t.messageId] })]);

export const userRoomLastRead = pgTable('user_room_last_read', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  lastReadMessageId: integer('last_read_message_id'),
  updatedAt: timestamp('updated_at').defaultNow().notNull(),
}, (t) => [primaryKey({ columns: [t.userId, t.roomId] })]);

export const messageReactions = pgTable('message_reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
}, (t) => [unique().on(t.messageId, t.userId, t.emoji)]);

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  scheduledAt: timestamp('scheduled_at').notNull(),
  sentAt: timestamp('sent_at'),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  previousContent: text('previous_content').notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
});
