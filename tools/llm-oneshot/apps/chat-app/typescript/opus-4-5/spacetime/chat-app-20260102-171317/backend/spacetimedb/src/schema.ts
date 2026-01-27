import { schema, table, t } from 'spacetimedb/server';
import { Timestamp, ScheduleAt } from 'spacetimedb';

// ============================================================================
// USER TABLES
// ============================================================================

// User table - stores user identity and profile
export const User = table(
  {
    name: 'user',
    public: true,
    indexes: [{ name: 'by_name', algorithm: 'btree', columns: ['name'] }],
  },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    online: t.bool(),
    // Rich presence: 'online' | 'away' | 'dnd' | 'invisible'
    status: t.string(),
    lastActiveAt: t.timestamp().optional(),
    connectionId: t.connectionId().optional(),
  }
);

// ============================================================================
// ROOM TABLES
// ============================================================================

// Room table - chat rooms
export const Room = table(
  {
    name: 'room',
    public: true,
    indexes: [{ name: 'by_owner', algorithm: 'btree', columns: ['ownerId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    ownerId: t.identity(),
    isPrivate: t.bool(),
    isDm: t.bool(),
    createdAt: t.timestamp(),
  }
);

// Room membership - who is in which room
export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    isAdmin: t.bool(),
    joinedAt: t.timestamp(),
    lastReadMessageId: t.u64().optional(),
    lastReadAt: t.timestamp().optional(),
  }
);

// Banned users from rooms
export const BannedUser = table(
  {
    name: 'banned_user',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    bannedBy: t.identity(),
    bannedAt: t.timestamp(),
    reason: t.string().optional(),
  }
);

// Room invitations for private rooms
export const RoomInvitation = table(
  {
    name: 'room_invitation',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_invitee', algorithm: 'btree', columns: ['inviteeId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    inviterId: t.identity(),
    inviteeId: t.identity(),
    // 'pending' | 'accepted' | 'declined'
    status: t.string(),
    createdAt: t.timestamp(),
  }
);

// ============================================================================
// MESSAGE TABLES
// ============================================================================

// Message table - chat messages
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_sender', algorithm: 'btree', columns: ['senderId'] },
      { name: 'by_parent', algorithm: 'btree', columns: ['parentMessageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
    createdAt: t.timestamp(),
    editedAt: t.timestamp().optional(),
    isEdited: t.bool(),
    // For threading
    parentMessageId: t.u64().optional(),
    replyCount: t.u32(),
    // For ephemeral messages
    expiresAt: t.timestamp().optional(),
  }
);

// Message edit history
export const MessageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    previousContent: t.string(),
    editedAt: t.timestamp(),
  }
);

// Message reactions
export const MessageReaction = table(
  {
    name: 'message_reaction',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userId: t.identity(),
    emoji: t.string(),
    createdAt: t.timestamp(),
  }
);

// Read receipts - tracks who has seen which messages
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userId: t.identity(),
    seenAt: t.timestamp(),
  }
);

// ============================================================================
// TYPING INDICATORS
// ============================================================================

// Typing indicator table
export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    startedAt: t.timestamp(),
    expiresAt: t.timestamp(),
  }
);

// Scheduled cleanup for typing indicators
export const TypingCleanupJob = table(
  {
    name: 'typing_cleanup_job',
    scheduled: 'run_typing_cleanup',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    typingIndicatorId: t.u64(),
  }
);

// ============================================================================
// SCHEDULED MESSAGES
// ============================================================================

// Scheduled messages - messages to be sent in the future
export const ScheduledMessage = table(
  {
    name: 'scheduled_message',
    scheduled: 'run_scheduled_message',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
    createdAt: t.timestamp(),
  }
);

// Public view for scheduled messages (users see only their own)
export const ScheduledMessageView = table(
  {
    name: 'scheduled_message_view',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_sender', algorithm: 'btree', columns: ['senderId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
    scheduledFor: t.timestamp(),
    createdAt: t.timestamp(),
  }
);

// ============================================================================
// EPHEMERAL MESSAGE CLEANUP
// ============================================================================

// Cleanup job for ephemeral messages
export const EphemeralCleanupJob = table(
  {
    name: 'ephemeral_cleanup_job',
    scheduled: 'run_ephemeral_cleanup',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ============================================================================
// PRESENCE CLEANUP
// ============================================================================

// Auto-away job for presence
export const PresenceAwayJob = table(
  {
    name: 'presence_away_job',
    scheduled: 'run_presence_away',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    userId: t.identity(),
  }
);

// ============================================================================
// EXPORT SCHEMA
// ============================================================================

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  BannedUser,
  RoomInvitation,
  Message,
  MessageEdit,
  MessageReaction,
  ReadReceipt,
  TypingIndicator,
  TypingCleanupJob,
  ScheduledMessage,
  ScheduledMessageView,
  EphemeralCleanupJob,
  PresenceAwayJob
);
