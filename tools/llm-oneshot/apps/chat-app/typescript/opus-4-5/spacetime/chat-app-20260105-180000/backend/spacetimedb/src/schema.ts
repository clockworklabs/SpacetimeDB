import { schema, table, t } from 'spacetimedb/server';

// ============================================================================
// USER TABLES
// ============================================================================

// User profiles with presence
export const User = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    online: t.bool(),
    status: t.string(), // 'online' | 'away' | 'dnd' | 'invisible'
    lastActive: t.timestamp(),
    connectionId: t.u64().optional(),
  }
);

// ============================================================================
// ROOM TABLES
// ============================================================================

// Rooms can be public or private (invite-only)
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
    creatorIdentity: t.identity(),
    createdAt: t.timestamp(),
    isPrivate: t.bool(), // Private rooms don't show in public list
    isDm: t.bool(), // Direct message rooms between two users
  }
);

// Room membership - who has joined which room
export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
    isAdmin: t.bool(),
    isBanned: t.bool(),
    joinedAt: t.timestamp(),
    lastReadMessageId: t.u64().optional(), // For tracking unread counts
  }
);

// Room invitations for private rooms
export const RoomInvitation = table(
  {
    name: 'room_invitation',
    public: true,
    indexes: [
      { name: 'by_invitee', algorithm: 'btree', columns: ['inviteeIdentity'] },
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    inviterIdentity: t.identity(),
    inviteeIdentity: t.identity(),
    createdAt: t.timestamp(),
    status: t.string(), // 'pending' | 'accepted' | 'declined'
  }
);

// ============================================================================
// MESSAGE TABLES
// ============================================================================

// Main messages table
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_sender', algorithm: 'btree', columns: ['senderIdentity'] },
      { name: 'by_parent', algorithm: 'btree', columns: ['parentMessageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    content: t.string(),
    createdAt: t.timestamp(),
    isEdited: t.bool(),
    parentMessageId: t.u64().optional(), // For threading - null means top-level message
    isEphemeral: t.bool(), // Disappearing messages
    expiresAt: t.timestamp().optional(), // When ephemeral message expires
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
export const Reaction = table(
  {
    name: 'reaction',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

// Read receipts
export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_user', algorithm: 'btree', columns: ['userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    readAt: t.timestamp(),
  }
);

// ============================================================================
// TYPING INDICATORS (public - client filters by room membership)
// ============================================================================

export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    scheduled: 'expire_typing',
    indexes: [{ name: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    userIdentity: t.identity(),
  }
);

// ============================================================================
// SCHEDULED MESSAGES (public - client filters by owner)
// ============================================================================

export const ScheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: 'send_scheduled_message',
    indexes: [
      { name: 'by_owner', algorithm: 'btree', columns: ['ownerIdentity'] },
    ],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    ownerIdentity: t.identity(),
    content: t.string(),
  }
);

// ============================================================================
// EPHEMERAL MESSAGE CLEANUP
// ============================================================================

export const EphemeralMessageCleanup = table(
  {
    name: 'ephemeral_message_cleanup',
    scheduled: 'delete_ephemeral_message',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ============================================================================
// AUTO AWAY STATUS
// ============================================================================

export const AwayStatusJob = table(
  {
    name: 'away_status_job',
    scheduled: 'check_away_status',
    indexes: [
      { name: 'by_user', algorithm: 'btree', columns: ['userIdentity'] },
    ],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    userIdentity: t.identity(),
  }
);

// ============================================================================
// SCHEMA EXPORT
// ============================================================================

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  RoomInvitation,
  Message,
  MessageEdit,
  Reaction,
  ReadReceipt,
  TypingIndicator,
  ScheduledMessage,
  EphemeralMessageCleanup,
  AwayStatusJob
);
