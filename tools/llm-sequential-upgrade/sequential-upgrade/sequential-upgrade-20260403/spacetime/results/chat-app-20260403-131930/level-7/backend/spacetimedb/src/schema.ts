import { schema, table, t } from 'spacetimedb/server';

// Users table
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(),        // 'online' | 'away' | 'dnd' | 'invisible'
    lastActiveAt: t.timestamp(),
  }
);

// Rooms table
const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().unique(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

// Room memberships
const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    isAdmin: t.bool(),
  }
);

// Banned users per room (kicked users cannot rejoin)
const roomBan = table(
  { name: 'room_ban', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
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
    expiresAtMicros: t.u64(), // 0 = permanent; otherwise unix epoch micros when message auto-deletes
  }
);

// Typing indicators
const typingIndicator = table(
  { name: 'typing_indicator', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    updatedAt: t.timestamp(),
  }
);

// Read receipts - tracks the last message ID each user has read in each room
const readReceipt = table(
  { name: 'read_receipt', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    lastReadMessageId: t.u64(),
  }
);

// Scheduled messages - will be sent at the specified time
// scheduledMessage and sendScheduledMessage must be in the same file to avoid
// circular dependency ((): any => breaks the circular reference at runtime)
export const scheduledMessage = table({
  name: 'scheduled_message',
  public: true,
  scheduled: (): any => sendScheduledMessage,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  roomId: t.u64().index('btree'),
  sender: t.identity().index('btree'),
  text: t.string(),
});

// Scheduled table for per-message expiry timers (internal, not public)
export const messageExpiryTimer = table({
  name: 'message_expiry_timer',
  scheduled: (): any => deleteExpiredMessage,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  messageId: t.u64(),
});

// Message reactions
const messageReaction = table(
  { name: 'message_reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    emoji: t.string(),
  }
);

// Message edit history - stores previous versions when a message is edited
const messageEdit = table(
  { name: 'message_edit', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    editedBy: t.identity(),
    oldText: t.string(),
    newText: t.string(),
    editedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ user, room, roomMember, roomBan, message, typingIndicator, readReceipt, scheduledMessage, messageExpiryTimer, messageReaction, messageEdit });
export default spacetimedb;

// Called automatically when scheduled time arrives (must be in same file as scheduledMessage)
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Check the room still exists and sender is still a member
    const room = ctx.db.room.id.find(timer.roomId);
    if (!room) return;
    const members = [...ctx.db.roomMember.roomId.filter(timer.roomId)];
    const isMember = members.some(m => m.userIdentity.toHexString() === timer.sender.toHexString());
    if (!isMember) return;
    ctx.db.message.insert({ id: 0n, roomId: timer.roomId, sender: timer.sender, text: timer.text, sentAt: ctx.timestamp, expiresAtMicros: 0n });
  }
);

// Called automatically when a message's expiry timer fires
export const deleteExpiredMessage = spacetimedb.reducer(
  { timer: messageExpiryTimer.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
  }
);
