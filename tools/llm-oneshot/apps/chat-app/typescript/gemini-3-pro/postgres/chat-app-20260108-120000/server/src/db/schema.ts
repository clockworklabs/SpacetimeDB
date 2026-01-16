import { pgTable, serial, text, timestamp, uuid, boolean, integer, primaryKey } from 'drizzle-orm/pg-core';
import { relations } from 'drizzle-orm';

export const users = pgTable('users', {
  id: uuid('id').defaultRandom().primaryKey(),
  username: text('username').notNull().unique(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  scheduledFor: timestamp('scheduled_for'),
  expiresAt: timestamp('expires_at'),
  editedAt: timestamp('edited_at'),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id, { onDelete: 'cascade' }).notNull(),
  content: text('content').notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
});

export const reactions = pgTable('reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  emoji: text('emoji').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
});

export const messageReads = pgTable('message_reads', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  lastReadMessageId: integer('last_read_message_id'),
  updatedAt: timestamp('updated_at').defaultNow().notNull(),
});

// Relations
export const usersRelations = relations(users, ({ many }) => ({
  messages: many(messages),
  reactions: many(reactions),
  messageReads: many(messageReads),
}));

export const roomsRelations = relations(rooms, ({ many }) => ({
  messages: many(messages),
  messageReads: many(messageReads),
}));

export const messagesRelations = relations(messages, ({ one, many }) => ({
  author: one(users, {
    fields: [messages.userId],
    references: [users.id],
  }),
  room: one(rooms, {
    fields: [messages.roomId],
    references: [rooms.id],
  }),
  edits: many(messageEdits),
  reactions: many(reactions),
}));

export const messageEditsRelations = relations(messageEdits, ({ one }) => ({
  message: one(messages, {
    fields: [messageEdits.messageId],
    references: [messages.id],
  }),
}));

export const reactionsRelations = relations(reactions, ({ one }) => ({
  message: one(messages, {
    fields: [reactions.messageId],
    references: [messages.id],
  }),
  user: one(users, {
    fields: [reactions.userId],
    references: [users.id],
  }),
}));
