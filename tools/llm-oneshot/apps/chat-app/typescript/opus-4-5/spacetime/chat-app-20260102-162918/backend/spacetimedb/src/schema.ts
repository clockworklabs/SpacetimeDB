import { schema, table, t } from 'spacetimedb/server';

// ============================================================================
// USER MANAGEMENT
// ============================================================================

// User status enum values
export const UserStatus = {
  ONLINE: 'online',
  AWAY: 'away',
  DND: 'do_not_disturb',
  INVISIBLE: 'invisible',
  OFFLINE: 'offline',
} as const;

export const User = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    status: t.string(), // online, away, do_not_disturb, invisible, offline
    lastActive: t.timestamp(),
    online: t.bool(),
  }
);

// ============================================================================
// ROOMS
// ============================================================================

export const Room = table(
  {
    name: 'room',
    public: true,
    indexes: [
      { name: 'by_is_private', algorithm: 'btree', columns: ['isPrivate'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
    isPrivate: t.bool(), // Private rooms don't appear in public list
    isDm: t.bool(), // Direct message room between two users
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
  }
);

// Room bans
export const RoomBan = table(
  {
    name: 'room_ban',
    public: true,
    indexes: [{ name: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    bannedBy: t.identity(),
    bannedAt: t.timestamp(),
  }
);

// Room invitations for private rooms
export const RoomInvite = table(
  {
    name: 'room_invite',
    public: true,
    indexes: [
      { name: 'by_invitee', algorithm: 'btree', columns: ['inviteeId'] },
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    inviterId: t.identity(),
    inviteeId: t.identity(),
    createdAt: t.timestamp(),
    status: t.string(), // pending, accepted, declined
  }
);

// ============================================================================
// MESSAGES
// ============================================================================

export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_sender', algorithm: 'btree', columns: ['senderId'] },
      {
        name: 'by_thread_parent',
        algorithm: 'btree',
        columns: ['threadParentId'],
      },
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
    threadParentId: t.u64().optional(), // For threaded replies
    expiresAt: t.timestamp().optional(), // For ephemeral messages
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

// Read receipts - tracks which messages users have seen
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_user_room', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    roomId: t.u64(),
    userId: t.identity(),
    readAt: t.timestamp(),
  }
);

// ============================================================================
// TYPING INDICATORS
// ============================================================================

export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [{ name: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    startedAt: t.timestamp(),
  }
);

// Scheduled job to expire typing indicators
export const TypingExpiry = table(
  { name: 'typing_expiry', scheduled: 'expire_typing' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    typingIndicatorId: t.u64(),
  }
);

// ============================================================================
// SCHEDULED MESSAGES
// ============================================================================

export const ScheduledMessage = table(
  { name: 'scheduled_message', scheduled: 'send_scheduled_message' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
  }
);

// ============================================================================
// EPHEMERAL MESSAGE CLEANUP
// ============================================================================

export const EphemeralMessageCleanup = table(
  { name: 'ephemeral_message_cleanup', scheduled: 'cleanup_ephemeral_message' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ============================================================================
// AUTO-AWAY STATUS
// ============================================================================

export const AutoAwayCheck = table(
  { name: 'auto_away_check', scheduled: 'check_auto_away' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    userId: t.identity(),
  }
);

// ============================================================================
// SCHEMA EXPORT
// ============================================================================

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  RoomBan,
  RoomInvite,
  Message,
  MessageEdit,
  MessageReaction,
  ReadReceipt,
  TypingIndicator,
  TypingExpiry,
  ScheduledMessage,
  EphemeralMessageCleanup,
  AutoAwayCheck
);
