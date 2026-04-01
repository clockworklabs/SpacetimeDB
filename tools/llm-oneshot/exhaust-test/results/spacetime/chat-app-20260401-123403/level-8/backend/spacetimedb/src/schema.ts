import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// Users
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(),       // 'online' | 'away' | 'dnd' | 'invisible'
    lastActiveAt: t.timestamp(),
  }
);

// Chat rooms
const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Room membership
const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    joinedAt: t.timestamp(),
  }
);

// Messages
const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    expiresAtMicros: t.u64(), // 0 = never expires; >0 = micros since Unix epoch when it auto-deletes
  }
);

// Typing indicators (expiresAtMicros = microseconds since Unix epoch)
const typingState = table(
  { name: 'typing_state', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity(),
    expiresAtMicros: t.u64(),
  }
);

// Read state: last message read per user per room (for read receipts + unread counts)
const userRoomState = table(
  { name: 'user_room_state', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    userIdentity: t.identity().index('btree'),
    roomId: t.u64().index('btree'),
    lastReadMessageId: t.u64(),
  }
);

// Scheduled timer for cleaning up expired typing indicators (runs every 1s)
const typingCleanupTimer = table(
  {
    name: 'typing_cleanup_timer',
    scheduled: (): any => cleanupTyping,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

// Scheduled messages — fires once at the specified time to deliver the message
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64().index('btree'),
    sender: t.identity().index('btree'),
    text: t.string(),
  }
);

// Message reactions
const reaction = table(
  { name: 'reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    emoji: t.string(),
  }
);

// Message edit history
const messageEdit = table(
  { name: 'message_edit', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    editedAt: t.timestamp(),
    oldText: t.string(),
    newText: t.string(),
  }
);

// Room permissions: role = 'admin' | 'banned'
const roomPermission = table(
  { name: 'room_permission', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    role: t.string(), // 'admin' or 'banned'
  }
);

// Thread replies — replies to a specific parent message
const threadReply = table(
  { name: 'thread_reply', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    parentMessageId: t.u64().index('btree'),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
  }
);

const spacetimedb = schema({ user, room, roomMember, message, typingState, userRoomState, typingCleanupTimer, scheduledMessage, reaction, messageEdit, roomPermission, threadReply });
export default spacetimedb;

// Schedule the typing cleanup to run every 1 second
export const init = spacetimedb.init((ctx) => {
  ctx.db.typingCleanupTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(1_000_000n),
  });
});

// Cleanup expired typing indicators and ephemeral messages
export const cleanupTyping = spacetimedb.reducer(
  { timer: typingCleanupTimer.rowType },
  (ctx, { timer: _timer }) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const row of [...ctx.db.typingState.iter()]) {
      if (row.expiresAtMicros <= now) {
        ctx.db.typingState.id.delete(row.id);
      }
    }
    // Delete ephemeral messages that have expired
    for (const row of [...ctx.db.message.iter()]) {
      if (row.expiresAtMicros > 0n && row.expiresAtMicros <= now) {
        ctx.db.message.id.delete(row.id);
      }
    }
  }
);

// Deliver a scheduled message when its timer fires
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Verify the room still exists and sender is still a member
    if (!ctx.db.room.id.find(timer.roomId)) return;
    const isMember = [...ctx.db.roomMember.roomId.filter(timer.roomId)]
      .some(row => row.userIdentity.equals(timer.sender));
    if (!isMember) return;
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      sender: timer.sender,
      text: timer.text,
      sentAt: ctx.timestamp,
      expiresAtMicros: 0n,
    });
  }
);
