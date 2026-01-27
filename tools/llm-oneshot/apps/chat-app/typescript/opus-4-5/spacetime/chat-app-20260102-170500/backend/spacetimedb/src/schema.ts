import { table, t } from 'spacetimedb/server';

// ==================== USER ====================
export const User = table(
  {
    name: 'user',
    public: true,
  },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    status: t.string(), // 'online' | 'away' | 'dnd' | 'invisible'
    lastActive: t.timestamp(),
    online: t.bool(),
  }
);

// ==================== ROOM ====================
export const Room = table(
  {
    name: 'room',
    public: true,
    indexes: [
      { name: 'by_creator', algorithm: 'btree', columns: ['creatorId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    creatorId: t.identity(),
    isPrivate: t.bool(),
    isDm: t.bool(),
    createdAt: t.timestamp(),
  }
);

// ==================== ROOM MEMBER ====================
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
    role: t.string(), // 'admin' | 'member'
    joinedAt: t.timestamp(),
  }
);

// ==================== MESSAGE ====================
export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_sender', algorithm: 'btree', columns: ['senderId'] },
      { name: 'by_parent', algorithm: 'btree', columns: ['parentId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
    createdAt: t.timestamp(),
    editedAt: t.timestamp().optional(),
    parentId: t.u64().optional(), // For threading
    expiresAt: t.timestamp().optional(), // For ephemeral messages
  }
);

// ==================== TYPING INDICATOR ====================
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

// ==================== READ RECEIPT ====================
export const ReadReceipt = table(
  {
    name: 'read_receipt',
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
    lastReadMessageId: t.u64(),
    readAt: t.timestamp(),
  }
);

// ==================== REACTION ====================
export const Reaction = table(
  {
    name: 'reaction',
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
  }
);

// ==================== EDIT HISTORY ====================
export const EditHistory = table(
  {
    name: 'edit_history',
    public: true,
    indexes: [
      { name: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    oldContent: t.string(),
    editedAt: t.timestamp(),
  }
);

// ==================== ROOM INVITATION ====================
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
    status: t.string(), // 'pending' | 'accepted' | 'declined'
    createdAt: t.timestamp(),
  }
);

// ==================== SCHEDULED MESSAGE ====================
export const ScheduledMessage = table(
  {
    name: 'scheduled_message',
    scheduled: 'send_scheduled_message',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    senderId: t.identity(),
    content: t.string(),
  }
);

// ==================== EPHEMERAL MESSAGE CLEANUP ====================
export const EphemeralCleanup = table(
  {
    name: 'ephemeral_cleanup',
    scheduled: 'cleanup_ephemeral_message',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ==================== TYPING CLEANUP ====================
export const TypingCleanup = table(
  {
    name: 'typing_cleanup',
    scheduled: 'cleanup_typing_indicator',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    typingId: t.u64(),
  }
);
