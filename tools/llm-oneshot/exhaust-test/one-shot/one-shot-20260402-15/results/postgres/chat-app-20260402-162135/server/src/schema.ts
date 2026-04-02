import {
  pgTable,
  serial,
  varchar,
  text,
  integer,
  boolean,
  timestamp,
  primaryKey,
} from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: serial('id').primaryKey(),
  username: varchar('username', { length: 50 }).notNull().unique(),
  status: varchar('status', { length: 20 }).notNull().default('online'),
  lastActive: timestamp('last_active').notNull().defaultNow(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 100 }).notNull().unique(),
  creatorId: integer('creator_id')
    .notNull()
    .references(() => users.id),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export const roomMembers = pgTable(
  'room_members',
  {
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id),
    userId: integer('user_id')
      .notNull()
      .references(() => users.id),
    isAdmin: boolean('is_admin').notNull().default(false),
    isBanned: boolean('is_banned').notNull().default(false),
    joinedAt: timestamp('joined_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.roomId, t.userId] })],
);

export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id')
    .notNull()
    .references(() => rooms.id),
  userId: integer('user_id')
    .notNull()
    .references(() => users.id),
  content: text('content').notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
  expiresAt: timestamp('expires_at'),
  scheduledAt: timestamp('scheduled_at'),
  isSent: boolean('is_sent').notNull().default(true),
  isDeleted: boolean('is_deleted').notNull().default(false),
  editedAt: timestamp('edited_at'),
});

export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id')
    .notNull()
    .references(() => messages.id),
  oldContent: text('old_content').notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
});

export const readReceipts = pgTable(
  'read_receipts',
  {
    messageId: integer('message_id')
      .notNull()
      .references(() => messages.id),
    userId: integer('user_id')
      .notNull()
      .references(() => users.id),
    readAt: timestamp('read_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.messageId, t.userId] })],
);

export const userRoomReads = pgTable(
  'user_room_reads',
  {
    userId: integer('user_id')
      .notNull()
      .references(() => users.id),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id),
    lastReadAt: timestamp('last_read_at').notNull().defaultNow(),
  },
  (t) => [primaryKey({ columns: [t.userId, t.roomId] })],
);

export const reactions = pgTable('reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id')
    .notNull()
    .references(() => messages.id),
  userId: integer('user_id')
    .notNull()
    .references(() => users.id),
  emoji: varchar('emoji', { length: 10 }).notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});
