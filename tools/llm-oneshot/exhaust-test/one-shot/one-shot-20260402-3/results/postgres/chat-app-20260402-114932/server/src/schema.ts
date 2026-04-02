import {
  pgTable, serial, text, integer, timestamp, boolean, varchar, primaryKey,
} from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 50 }).notNull().unique(),
  status: varchar('status', { length: 20 }).notNull().default('online'),
  lastActive: timestamp('last_active').defaultNow().notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 100 }).notNull().unique(),
  createdBy: integer('created_by').references(() => users.id).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const roomMembers = pgTable('room_members', {
  roomId: integer('room_id').references(() => rooms.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  isAdmin: boolean('is_admin').default(false).notNull(),
  isBanned: boolean('is_banned').default(false).notNull(),
  joinedAt: timestamp('joined_at').defaultNow().notNull(),
}, (table) => ({
  pk: primaryKey({ columns: [table.roomId, table.userId] }),
}));

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  content: text('content').notNull(),
  isEdited: boolean('is_edited').default(false).notNull(),
  expiresAt: timestamp('expires_at'),
  deletedAt: timestamp('deleted_at'),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  updatedAt: timestamp('updated_at').defaultNow().notNull(),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id).notNull(),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
});

export const readReceipts = pgTable('read_receipts', {
  messageId: integer('message_id').references(() => messages.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  readAt: timestamp('read_at').defaultNow().notNull(),
}, (table) => ({
  pk: primaryKey({ columns: [table.messageId, table.userId] }),
}));

export const lastRead = pgTable('last_read', {
  roomId: integer('room_id').references(() => rooms.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  lastReadMessageId: integer('last_read_message_id'),
  updatedAt: timestamp('updated_at').defaultNow().notNull(),
}, (table) => ({
  pk: primaryKey({ columns: [table.roomId, table.userId] }),
}));

export const scheduledMessages = pgTable('scheduled_messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  content: text('content').notNull(),
  scheduledFor: timestamp('scheduled_for').notNull(),
  sent: boolean('sent').default(false).notNull(),
  cancelled: boolean('cancelled').default(false).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const reactions = pgTable('reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id).notNull(),
  userId: integer('user_id').references(() => users.id).notNull(),
  emoji: varchar('emoji', { length: 10 }).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});
