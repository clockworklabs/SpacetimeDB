import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// ── Users ──────────────────────────────────────────────────────────────────
export const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(), // 'online' | 'away' | 'dnd' | 'invisible'
    lastActive: t.timestamp(),
  }
);

// ── Rooms ──────────────────────────────────────────────────────────────────
export const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// ── Room Membership ────────────────────────────────────────────────────────
export const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    identity: t.identity().index('btree'),
    isAdmin: t.bool(),
    isBanned: t.bool(),
  }
);

// ── Messages ───────────────────────────────────────────────────────────────
export const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    isEphemeral: t.bool(),
    ephemeralDurationSecs: t.u64(), // seconds (0 = not ephemeral)
    isEdited: t.bool(),
    isDeleted: t.bool(),
  }
);

// ── Typing Indicators ──────────────────────────────────────────────────────
export const typingIndicator = table(
  { name: 'typing_indicator', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    identity: t.identity().index('btree'),
    expiresAtMicros: t.u64(), // microseconds since unix epoch
  }
);

// ── Read Receipts ──────────────────────────────────────────────────────────
export const messageRead = table(
  { name: 'message_read', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    identity: t.identity().index('btree'),
  }
);

// ── Room Read Positions (for unread counts) ────────────────────────────────
export const roomReadPosition = table(
  { name: 'room_read_position', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    identity: t.identity().index('btree'),
    lastReadMessageId: t.u64(),
  }
);

// ── Message Reactions ──────────────────────────────────────────────────────
export const messageReaction = table(
  { name: 'message_reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    identity: t.identity(),
    emoji: t.string(),
  }
);

// ── Message Edit History ───────────────────────────────────────────────────
export const messageEditHistory = table(
  { name: 'message_edit_history', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    text: t.string(),
    editedAt: t.timestamp(),
  }
);

// Note: Scheduled tables (scheduledMessage, ephemeralDeleteTimer,
// typingCleanupTimer, activityCheckTimer) are defined in index.ts
// alongside their reducers to avoid circular references.

export { ScheduleAt };
