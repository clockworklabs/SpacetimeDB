import { pgTable, serial, text, timestamp, integer, boolean, primaryKey } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  online: boolean('online').default(false).notNull(),
  lastSeen: timestamp('last_seen').defaultNow().notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const roomMembers = pgTable('room_members', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  joinedAt: timestamp('joined_at').defaultNow().notNull(),
}, (t) => [
  primaryKey({ columns: [t.userId, t.roomId] }),
]);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
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
