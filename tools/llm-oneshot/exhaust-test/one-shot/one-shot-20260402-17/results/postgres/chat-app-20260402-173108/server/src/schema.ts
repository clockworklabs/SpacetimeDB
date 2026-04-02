import { pgTable, serial, text, integer, boolean, timestamp, primaryKey, pgEnum } from 'drizzle-orm/pg-core';

export const statusEnum = pgEnum('user_status', ['online', 'away', 'dnd', 'invisible']);

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  status: statusEnum('status').notNull().default('online'),
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  creatorId: integer('creator_id').notNull().references(() => users.id),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable('room_members', {
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  isAdmin: boolean('is_admin').notNull().default(false),
  isBanned: boolean('is_banned').notNull().default(false),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
}, (t) => [primaryKey({ columns: [t.roomId, t.userId] })]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  isEdited: boolean('is_edited').notNull().default(false),
  isEphemeral: boolean('is_ephemeral').notNull().default(false),
  expiresAt: timestamp('expires_at'),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  updatedAt: timestamp('updated_at').notNull().defaultNow(),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
});

export const reactions = pgTable('reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const readReceipts = pgTable('read_receipts', {
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  lastReadMessageId: integer('last_read_message_id'),
  lastReadAt: timestamp('last_read_at').notNull().defaultNow(),
}, (t) => [primaryKey({ columns: [t.roomId, t.userId] })]);

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  scheduledAt: timestamp('scheduled_at').notNull(),
  sent: boolean('sent').notNull().default(false),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});
