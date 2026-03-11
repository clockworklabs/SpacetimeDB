import {
  boolean,
  index,
  integer,
  pgTable,
  primaryKey,
  serial,
  timestamp,
  unique,
  uuid,
  varchar,
} from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id: uuid('id').primaryKey(),
  displayName: varchar('display_name', { length: 40 }).notNull(),
  isOnline: boolean('is_online').notNull().default(false),
  lastActiveAt: timestamp('last_active_at', { withTimezone: true })
    .notNull()
    .defaultNow(),
  lastMessageAt: timestamp('last_message_at', { withTimezone: true }),
  createdAt: timestamp('created_at', { withTimezone: true })
    .notNull()
    .defaultNow(),
});

export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 64 }).notNull(),
  createdBy: uuid('created_by')
    .notNull()
    .references(() => users.id, { onDelete: 'cascade' }),
  createdAt: timestamp('created_at', { withTimezone: true })
    .notNull()
    .defaultNow(),
});

export const roomMembers = pgTable(
  'room_members',
  {
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    joinedAt: timestamp('joined_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  t => [
    primaryKey({ columns: [t.roomId, t.userId] }),
    index('room_members_room_id').on(t.roomId),
    index('room_members_user_id').on(t.userId),
  ]
);

export const messages = pgTable(
  'messages',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    authorId: uuid('author_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    content: varchar('content', { length: 2000 }).notNull(),
    createdAt: timestamp('created_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
    updatedAt: timestamp('updated_at', { withTimezone: true }),
    expiresAt: timestamp('expires_at', { withTimezone: true }),
  },
  t => [
    index('messages_room_id').on(t.roomId),
    index('messages_author_id').on(t.authorId),
  ]
);

export const messageEdits = pgTable(
  'message_edits',
  {
    id: serial('id').primaryKey(),
    messageId: integer('message_id')
      .notNull()
      .references(() => messages.id, { onDelete: 'cascade' }),
    editorId: uuid('editor_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    oldContent: varchar('old_content', { length: 2000 }).notNull(),
    newContent: varchar('new_content', { length: 2000 }).notNull(),
    editedAt: timestamp('edited_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  t => [index('message_edits_message_id').on(t.messageId)]
);

export const reactions = pgTable(
  'reactions',
  {
    id: serial('id').primaryKey(),
    messageId: integer('message_id')
      .notNull()
      .references(() => messages.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    emoji: varchar('emoji', { length: 16 }).notNull(),
    createdAt: timestamp('created_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  t => [
    unique('reactions_unique').on(t.messageId, t.userId, t.emoji),
    index('reactions_message_id').on(t.messageId),
  ]
);

export const scheduledMessages = pgTable(
  'scheduled_messages',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    authorId: uuid('author_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    content: varchar('content', { length: 2000 }).notNull(),
    sendAt: timestamp('send_at', { withTimezone: true }).notNull(),
    createdAt: timestamp('created_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
    cancelledAt: timestamp('cancelled_at', { withTimezone: true }),
    sentAt: timestamp('sent_at', { withTimezone: true }),
  },
  t => [
    index('scheduled_messages_author_id').on(t.authorId),
    index('scheduled_messages_room_id').on(t.roomId),
    index('scheduled_messages_send_at').on(t.sendAt),
  ]
);

export const roomReadPositions = pgTable(
  'room_read_positions',
  {
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    lastReadMessageId: integer('last_read_message_id'),
    updatedAt: timestamp('updated_at', { withTimezone: true })
      .notNull()
      .defaultNow(),
  },
  t => [
    primaryKey({ columns: [t.roomId, t.userId] }),
    index('room_read_positions_room_id').on(t.roomId),
  ]
);

export type UserRow = typeof users.$inferSelect;
export type RoomRow = typeof rooms.$inferSelect;
export type RoomMemberRow = typeof roomMembers.$inferSelect;
export type MessageRow = typeof messages.$inferSelect;
export type MessageEditRow = typeof messageEdits.$inferSelect;
export type ReactionRow = typeof reactions.$inferSelect;
export type ScheduledMessageRow = typeof scheduledMessages.$inferSelect;
export type RoomReadPositionRow = typeof roomReadPositions.$inferSelect;
