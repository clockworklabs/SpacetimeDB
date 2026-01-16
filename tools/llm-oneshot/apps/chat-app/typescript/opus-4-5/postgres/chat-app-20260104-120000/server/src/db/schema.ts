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

// =====================
// USERS
// =====================
export const users = pgTable('users', {
  id: uuid('id').primaryKey().defaultRandom(),
  displayName: varchar('display_name', { length: 50 }).notNull(),
  status: varchar('status', { length: 20 }).notNull().default('online'), // online, away, dnd, invisible
  lastActiveAt: timestamp('last_active_at').notNull().defaultNow(),
  lastActionAt: timestamp('last_action_at'), // for rate limiting
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export type User = typeof users.$inferSelect;
export type InsertUser = typeof users.$inferInsert;

// =====================
// ROOMS
// =====================
export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 100 }).notNull(),
  isPrivate: boolean('is_private').notNull().default(false),
  isDm: boolean('is_dm').notNull().default(false),
  createdBy: uuid('created_by').notNull().references(() => users.id, { onDelete: 'cascade' }),
  createdAt: timestamp('created_at').notNull().defaultNow(),
});

export type Room = typeof rooms.$inferSelect;
export type InsertRoom = typeof rooms.$inferInsert;

// =====================
// ROOM MEMBERS
// =====================
export const roomMembers = pgTable('room_members', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  isAdmin: boolean('is_admin').notNull().default(false),
  isBanned: boolean('is_banned').notNull().default(false),
  lastReadAt: timestamp('last_read_at'),
  joinedAt: timestamp('joined_at').notNull().defaultNow(),
}, (table) => [
  unique().on(table.roomId, table.userId),
  index('room_members_room_idx').on(table.roomId),
  index('room_members_user_idx').on(table.userId),
]);

export type RoomMember = typeof roomMembers.$inferSelect;
export type InsertRoomMember = typeof roomMembers.$inferInsert;

// =====================
// ROOM INVITATIONS
// =====================
export const roomInvitations = pgTable('room_invitations', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  invitedUserId: uuid('invited_user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  invitedBy: uuid('invited_by').notNull().references(() => users.id, { onDelete: 'cascade' }),
  status: varchar('status', { length: 20 }).notNull().default('pending'), // pending, accepted, declined
  createdAt: timestamp('created_at').notNull().defaultNow(),
}, (table) => [
  unique().on(table.roomId, table.invitedUserId),
  index('invitations_user_idx').on(table.invitedUserId),
]);

export type RoomInvitation = typeof roomInvitations.$inferSelect;
export type InsertRoomInvitation = typeof roomInvitations.$inferInsert;

// =====================
// MESSAGES
// =====================
export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  content: varchar('content', { length: 2000 }).notNull(),
  isEdited: boolean('is_edited').notNull().default(false),
  replyToId: integer('reply_to_id'),
  scheduledFor: timestamp('scheduled_for'),
  expiresAt: timestamp('expires_at'),
  createdAt: timestamp('created_at').notNull().defaultNow(),
}, (table) => [
  index('messages_room_idx').on(table.roomId),
  index('messages_scheduled_idx').on(table.scheduledFor),
  index('messages_expires_idx').on(table.expiresAt),
  index('messages_reply_idx').on(table.replyToId),
]);

export type Message = typeof messages.$inferSelect;
export type InsertMessage = typeof messages.$inferInsert;

// =====================
// MESSAGE EDITS (History)
// =====================
export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  previousContent: varchar('previous_content', { length: 2000 }).notNull(),
  editedAt: timestamp('edited_at').notNull().defaultNow(),
}, (table) => [
  index('edits_message_idx').on(table.messageId),
]);

export type MessageEdit = typeof messageEdits.$inferSelect;
export type InsertMessageEdit = typeof messageEdits.$inferInsert;

// =====================
// MESSAGE REACTIONS
// =====================
export const messageReactions = pgTable('message_reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  emoji: varchar('emoji', { length: 10 }).notNull(),
  createdAt: timestamp('created_at').notNull().defaultNow(),
}, (table) => [
  unique().on(table.messageId, table.userId, table.emoji),
  index('reactions_message_idx').on(table.messageId),
]);

export type MessageReaction = typeof messageReactions.$inferSelect;
export type InsertMessageReaction = typeof messageReactions.$inferInsert;

// =====================
// READ RECEIPTS
// =====================
export const readReceipts = pgTable('read_receipts', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').notNull().references(() => messages.id, { onDelete: 'cascade' }),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  readAt: timestamp('read_at').notNull().defaultNow(),
}, (table) => [
  unique().on(table.messageId, table.userId),
  index('receipts_message_idx').on(table.messageId),
  index('receipts_user_idx').on(table.userId),
]);

export type ReadReceipt = typeof readReceipts.$inferSelect;
export type InsertReadReceipt = typeof readReceipts.$inferInsert;

// =====================
// TYPING INDICATORS
// =====================
export const typingIndicators = pgTable('typing_indicators', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').notNull().references(() => rooms.id, { onDelete: 'cascade' }),
  userId: uuid('user_id').notNull().references(() => users.id, { onDelete: 'cascade' }),
  startedAt: timestamp('started_at').notNull().defaultNow(),
}, (table) => [
  unique().on(table.roomId, table.userId),
  index('typing_room_idx').on(table.roomId),
]);

export type TypingIndicator = typeof typingIndicators.$inferSelect;
export type InsertTypingIndicator = typeof typingIndicators.$inferInsert;
