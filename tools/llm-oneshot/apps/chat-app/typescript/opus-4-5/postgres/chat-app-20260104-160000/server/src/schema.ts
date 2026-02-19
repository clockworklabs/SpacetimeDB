import {
  pgTable,
  uuid,
  varchar,
  text,
  timestamp,
  boolean,
  integer,
  serial,
  index,
  unique,
} from 'drizzle-orm/pg-core';

// Users table
export const users = pgTable(
  'users',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    displayName: varchar('display_name', { length: 50 }).notNull(),
    status: varchar('status', { length: 20 }).notNull().default('online'), // online, away, dnd, invisible
    lastActive: timestamp('last_active').notNull().defaultNow(),
    createdAt: timestamp('created_at').notNull().defaultNow(),
  },
  table => [index('users_display_name_idx').on(table.displayName)]
);

// Rooms table
export const rooms = pgTable(
  'rooms',
  {
    id: serial('id').primaryKey(),
    name: varchar('name', { length: 100 }).notNull(),
    isPrivate: boolean('is_private').notNull().default(false),
    isDm: boolean('is_dm').notNull().default(false),
    createdBy: uuid('created_by')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    createdAt: timestamp('created_at').notNull().defaultNow(),
  },
  table => [index('rooms_created_by_idx').on(table.createdBy)]
);

// Room members (for joining/membership)
export const roomMembers = pgTable(
  'room_members',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    role: varchar('role', { length: 20 }).notNull().default('member'), // admin, member
    isBanned: boolean('is_banned').notNull().default(false),
    lastReadAt: timestamp('last_read_at'),
    joinedAt: timestamp('joined_at').notNull().defaultNow(),
  },
  table => [
    index('room_members_room_id_idx').on(table.roomId),
    index('room_members_user_id_idx').on(table.userId),
    unique('room_members_unique').on(table.roomId, table.userId),
  ]
);

// Room invitations (for private rooms)
export const roomInvitations = pgTable(
  'room_invitations',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    invitedUserId: uuid('invited_user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    invitedBy: uuid('invited_by')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    status: varchar('status', { length: 20 }).notNull().default('pending'), // pending, accepted, declined
    createdAt: timestamp('created_at').notNull().defaultNow(),
  },
  table => [
    index('room_invitations_invited_user_idx').on(table.invitedUserId),
    unique('room_invitations_unique').on(table.roomId, table.invitedUserId),
  ]
);

// Messages table
export const messages = pgTable(
  'messages',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    content: varchar('content', { length: 2000 }).notNull(),
    isEdited: boolean('is_edited').notNull().default(false),
    parentMessageId: integer('parent_message_id'),
    isScheduled: boolean('is_scheduled').notNull().default(false),
    scheduledFor: timestamp('scheduled_for'),
    isEphemeral: boolean('is_ephemeral').notNull().default(false),
    expiresAt: timestamp('expires_at'),
    createdAt: timestamp('created_at').notNull().defaultNow(),
  },
  table => [
    index('messages_room_id_idx').on(table.roomId),
    index('messages_user_id_idx').on(table.userId),
    index('messages_parent_id_idx').on(table.parentMessageId),
    index('messages_scheduled_idx').on(table.scheduledFor),
    index('messages_expires_idx').on(table.expiresAt),
  ]
);

// Message edit history
export const messageEdits = pgTable(
  'message_edits',
  {
    id: serial('id').primaryKey(),
    messageId: integer('message_id')
      .notNull()
      .references(() => messages.id, { onDelete: 'cascade' }),
    previousContent: varchar('previous_content', { length: 2000 }).notNull(),
    editedAt: timestamp('edited_at').notNull().defaultNow(),
  },
  table => [index('message_edits_message_id_idx').on(table.messageId)]
);

// Message reactions
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
    emoji: varchar('emoji', { length: 10 }).notNull(),
    createdAt: timestamp('created_at').notNull().defaultNow(),
  },
  table => [
    index('reactions_message_id_idx').on(table.messageId),
    unique('reactions_unique').on(table.messageId, table.userId, table.emoji),
  ]
);

// Read receipts
export const readReceipts = pgTable(
  'read_receipts',
  {
    id: serial('id').primaryKey(),
    messageId: integer('message_id')
      .notNull()
      .references(() => messages.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    readAt: timestamp('read_at').notNull().defaultNow(),
  },
  table => [
    index('read_receipts_message_id_idx').on(table.messageId),
    unique('read_receipts_unique').on(table.messageId, table.userId),
  ]
);

// Typing indicators
export const typingIndicators = pgTable(
  'typing_indicators',
  {
    id: serial('id').primaryKey(),
    roomId: integer('room_id')
      .notNull()
      .references(() => rooms.id, { onDelete: 'cascade' }),
    userId: uuid('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    expiresAt: timestamp('expires_at').notNull(),
  },
  table => [
    index('typing_indicators_room_id_idx').on(table.roomId),
    unique('typing_indicators_unique').on(table.roomId, table.userId),
  ]
);

// Type exports
export type User = typeof users.$inferSelect;
export type NewUser = typeof users.$inferInsert;
export type Room = typeof rooms.$inferSelect;
export type NewRoom = typeof rooms.$inferInsert;
export type RoomMember = typeof roomMembers.$inferSelect;
export type Message = typeof messages.$inferSelect;
export type Reaction = typeof reactions.$inferSelect;
export type ReadReceipt = typeof readReceipts.$inferSelect;
export type RoomInvitation = typeof roomInvitations.$inferSelect;
