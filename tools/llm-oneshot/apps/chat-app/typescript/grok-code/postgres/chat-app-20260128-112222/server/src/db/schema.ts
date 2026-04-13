import {
  pgTable,
  text,
  timestamp,
  uuid,
  integer,
  boolean,
  jsonb,
  index,
  uniqueIndex,
} from 'drizzle-orm/pg-core';

// Users table
export const users = pgTable('users', {
  id: uuid('id').primaryKey().defaultRandom(),
  displayName: text('display_name').notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  lastSeen: timestamp('last_seen').defaultNow().notNull(),
});

// Chat rooms
export const rooms = pgTable('rooms', {
  id: uuid('id').primaryKey().defaultRandom(),
  name: text('name').notNull(),
  createdBy: uuid('created_by')
    .references(() => users.id)
    .notNull(),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  isPrivate: boolean('is_private').default(false).notNull(),
});

// Room members
export const roomMembers = pgTable(
  'room_members',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    roomId: uuid('room_id')
      .references(() => rooms.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    joinedAt: timestamp('joined_at').defaultNow().notNull(),
    lastReadMessageId: uuid('last_read_message_id').references(
      () => messages.id
    ),
  },
  table => ({
    roomUserIdx: index('room_members_room_user_idx').on(
      table.roomId,
      table.userId
    ),
  })
);

// Messages
export const messages = pgTable(
  'messages',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    roomId: uuid('room_id')
      .references(() => rooms.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    content: text('content').notNull(),
    createdAt: timestamp('created_at').defaultNow().notNull(),
    updatedAt: timestamp('updated_at').defaultNow().notNull(),
    expiresAt: timestamp('expires_at'), // For ephemeral messages
    scheduledFor: timestamp('scheduled_for'), // For scheduled messages
    isDeleted: boolean('is_deleted').default(false).notNull(),
  },
  table => ({
    roomCreatedIdx: index('messages_room_created_idx').on(
      table.roomId,
      table.createdAt
    ),
    scheduledIdx: index('messages_scheduled_idx').on(table.scheduledFor),
    expiresIdx: index('messages_expires_idx').on(table.expiresAt),
  })
);

// Message edit history
export const messageEdits = pgTable('message_edits', {
  id: uuid('id').primaryKey().defaultRandom(),
  messageId: uuid('message_id')
    .references(() => messages.id)
    .notNull(),
  previousContent: text('previous_content').notNull(),
  editedAt: timestamp('edited_at').defaultNow().notNull(),
  editedBy: uuid('edited_by')
    .references(() => users.id)
    .notNull(),
});

// Message reactions
export const messageReactions = pgTable(
  'message_reactions',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    messageId: uuid('message_id')
      .references(() => messages.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    emoji: text('emoji').notNull(),
    createdAt: timestamp('created_at').defaultNow().notNull(),
  },
  table => ({
    messageUserIdx: index('message_reactions_message_user_idx').on(
      table.messageId,
      table.userId
    ),
  })
);

// Read receipts
export const readReceipts = pgTable(
  'read_receipts',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    messageId: uuid('message_id')
      .references(() => messages.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    readAt: timestamp('read_at').defaultNow().notNull(),
  },
  table => ({
    messageUserIdx: index('read_receipts_message_user_idx').on(
      table.messageId,
      table.userId
    ),
  })
);

// Typing indicators
export const typingIndicators = pgTable(
  'typing_indicators',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    roomId: uuid('room_id')
      .references(() => rooms.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    startedAt: timestamp('started_at').defaultNow().notNull(),
    expiresAt: timestamp('expires_at').notNull(), // Auto-expire after inactivity
  },
  table => ({
    roomUserIdx: uniqueIndex('typing_indicators_room_user_idx').on(
      table.roomId,
      table.userId
    ),
  })
);

// Unread message counts (computed view, but we'll use a table for performance)
export const unreadCounts = pgTable(
  'unread_counts',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    roomId: uuid('room_id')
      .references(() => rooms.id)
      .notNull(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    count: integer('count').default(0).notNull(),
    updatedAt: timestamp('updated_at').defaultNow().notNull(),
  },
  table => ({
    roomUserIdx: uniqueIndex('unread_counts_room_user_idx').on(
      table.roomId,
      table.userId
    ),
  })
);

// Scheduled messages queue
export const scheduledMessages = pgTable(
  'scheduled_messages',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    messageId: uuid('message_id')
      .references(() => messages.id)
      .notNull(),
    scheduledFor: timestamp('scheduled_for').notNull(),
    status: text('status').default('pending').notNull(), // pending, sent, cancelled
    createdAt: timestamp('created_at').defaultNow().notNull(),
  },
  table => ({
    scheduledIdx: index('scheduled_messages_scheduled_idx').on(
      table.scheduledFor
    ),
  })
);

// Online users tracking
export const onlineUsers = pgTable(
  'online_users',
  {
    id: uuid('id').primaryKey().defaultRandom(),
    userId: uuid('user_id')
      .references(() => users.id)
      .notNull(),
    socketId: text('socket_id').notNull(),
    connectedAt: timestamp('connected_at').defaultNow().notNull(),
    lastPing: timestamp('last_ping').defaultNow().notNull(),
  },
  table => ({
    userIdx: index('online_users_user_idx').on(table.userId),
    socketIdx: index('online_users_socket_idx').on(table.socketId),
  })
);
