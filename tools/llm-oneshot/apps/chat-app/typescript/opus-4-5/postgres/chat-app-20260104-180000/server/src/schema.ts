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
  pgEnum,
} from 'drizzle-orm/pg-core';

// Enums
export const userStatusEnum = pgEnum('user_status', ['online', 'away', 'dnd', 'invisible', 'offline']);
export const roomTypeEnum = pgEnum('room_type', ['public', 'private', 'dm']);
export const memberRoleEnum = pgEnum('member_role', ['member', 'admin']);
export const inviteStatusEnum = pgEnum('invite_status', ['pending', 'accepted', 'declined']);

// Users table
export const users = pgTable('users', {
  id: uuid('id').primaryKey().defaultRandom(),
  displayName: varchar('display_name', { length: 50 }).notNull(),
  status: userStatusEnum('status').default('online').notNull(),
  lastActiveAt: timestamp('last_active_at').defaultNow().notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  lastMessageAt: timestamp('last_message_at'),
}, (table) => [
  index('users_status_idx').on(table.status),
]);

// Rooms table
export const rooms = pgTable('rooms', {
  id: serial('id').primaryKey(),
  name: varchar('name', { length: 100 }).notNull(),
  createdBy: uuid('created_by').references(() => users.id, { onDelete: 'set null' }),
  roomType: roomTypeEnum('room_type').default('public').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
}, (table) => [
  index('rooms_type_idx').on(table.roomType),
  index('rooms_created_by_idx').on(table.createdBy),
]);

// Room members table
export const roomMembers = pgTable('room_members', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  role: memberRoleEnum('role').default('member').notNull(),
  isBanned: boolean('is_banned').default(false).notNull(),
  joinedAt: timestamp('joined_at').defaultNow().notNull(),
  lastReadAt: timestamp('last_read_at').defaultNow().notNull(),
}, (table) => [
  unique('room_members_unique').on(table.roomId, table.userId),
  index('room_members_room_idx').on(table.roomId),
  index('room_members_user_idx').on(table.userId),
]);

// Room invites table
export const roomInvites = pgTable('room_invites', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  invitedBy: uuid('invited_by').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  invitedUser: uuid('invited_user').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  status: inviteStatusEnum('status').default('pending').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
}, (table) => [
  unique('room_invites_unique').on(table.roomId, table.invitedUser),
  index('room_invites_user_idx').on(table.invitedUser),
]);

// Messages table
export const messages = pgTable('messages', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  content: varchar('content', { length: 2000 }).notNull(),
  parentId: integer('parent_id'),
  isEdited: boolean('is_edited').default(false).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  // For scheduled messages
  scheduledFor: timestamp('scheduled_for'),
  isScheduled: boolean('is_scheduled').default(false).notNull(),
  // For ephemeral messages
  expiresAt: timestamp('expires_at'),
}, (table) => [
  index('messages_room_idx').on(table.roomId),
  index('messages_user_idx').on(table.userId),
  index('messages_parent_idx').on(table.parentId),
  index('messages_scheduled_idx').on(table.isScheduled, table.scheduledFor),
  index('messages_expires_idx').on(table.expiresAt),
]);

// Message edits history
export const messageEdits = pgTable('message_edits', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id, { onDelete: 'cascade' }).notNull(),
  previousContent: varchar('previous_content', { length: 2000 }).notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
}, (table) => [
  index('message_edits_message_idx').on(table.messageId),
]);

// Message reactions
export const messageReactions = pgTable('message_reactions', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  emoji: varchar('emoji', { length: 10 }).notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
}, (table) => [
  unique('message_reactions_unique').on(table.messageId, table.userId, table.emoji),
  index('message_reactions_message_idx').on(table.messageId),
]);

// Read receipts
export const readReceipts = pgTable('read_receipts', {
  id: serial('id').primaryKey(),
  messageId: integer('message_id').references(() => messages.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  readAt: timestamp('read_at').defaultNow().notNull(),
}, (table) => [
  unique('read_receipts_unique').on(table.messageId, table.userId),
  index('read_receipts_message_idx').on(table.messageId),
  index('read_receipts_user_idx').on(table.userId),
]);

// Typing indicators (stored in memory, but we keep a table for cleanup)
export const typingIndicators = pgTable('typing_indicators', {
  id: serial('id').primaryKey(),
  roomId: integer('room_id').references(() => rooms.id, { onDelete: 'cascade' }).notNull(),
  userId: uuid('user_id').references(() => users.id, { onDelete: 'cascade' }).notNull(),
  startedAt: timestamp('started_at').defaultNow().notNull(),
  expiresAt: timestamp('expires_at').notNull(),
}, (table) => [
  unique('typing_indicators_unique').on(table.roomId, table.userId),
  index('typing_indicators_room_idx').on(table.roomId),
  index('typing_indicators_expires_idx').on(table.expiresAt),
]);

// Type exports
export type User = typeof users.$inferSelect;
export type NewUser = typeof users.$inferInsert;
export type Room = typeof rooms.$inferSelect;
export type NewRoom = typeof rooms.$inferInsert;
export type RoomMember = typeof roomMembers.$inferSelect;
export type Message = typeof messages.$inferSelect;
export type MessageEdit = typeof messageEdits.$inferSelect;
export type MessageReaction = typeof messageReactions.$inferSelect;
export type ReadReceipt = typeof readReceipts.$inferSelect;
export type RoomInvite = typeof roomInvites.$inferSelect;
export type TypingIndicator = typeof typingIndicators.$inferSelect;
