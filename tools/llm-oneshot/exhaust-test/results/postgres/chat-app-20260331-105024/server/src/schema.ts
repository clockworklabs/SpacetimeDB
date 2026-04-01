import { pgTable, serial, text, integer, boolean, timestamp, primaryKey } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  online: boolean('online').notNull().default(false),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: text('name').notNull().unique(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable('room_members', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
}, (t) => ({
  pk: primaryKey({ columns: [t.userId, t.roomId] }),
}));

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const lastRead = pgTable('last_read', {
  userId: integer('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  lastMessageId: integer('last_message_id'),
  updatedAt: timestamp('updated_at').notNull().defaultNow(),
}, (t) => ({
  pk: primaryKey({ columns: [t.userId, t.roomId] }),
}));
