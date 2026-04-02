import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// ============================================================
// Table Definitions
// ============================================================

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(), // 'online' | 'away' | 'dnd' | 'invisible'
    lastActive: t.timestamp(),
  }
);

const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    identity: t.identity().index('btree'),
    isAdmin: t.bool(),
    isBanned: t.bool(),
  }
);

const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    editedAt: t.option(t.timestamp()),
    isEphemeral: t.bool(),
    expiresAt: t.option(t.timestamp()),
    isDeleted: t.bool(),
  }
);

const messageEdit = table(
  { name: 'message_edit', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    text: t.string(),
    editedAt: t.timestamp(),
  }
);

const typingIndicator = table(
  { name: 'typing_indicator', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userId: t.identity(),
    expiresAt: t.timestamp(),
  }
);

const readReceipt = table(
  { name: 'read_receipt', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userId: t.identity(),
    seenAt: t.timestamp(),
  }
);

const userRoomRead = table(
  { name: 'user_room_read', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    userId: t.identity().index('btree'),
    roomId: t.u64(),
    lastReadMessageId: t.u64(),
  }
);

const reaction = table(
  { name: 'reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userId: t.identity(),
    emoji: t.string(),
  }
);

// Scheduled table for user-scheduled messages
// Forward ref to sendScheduledMessage reducer (defined below)
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
  }
);

// Internal cleanup timer — fires every 10 seconds
// Forward ref to runCleanup reducer (defined below)
const cleanupTimer = table(
  {
    name: 'cleanup_timer',
    scheduled: (): any => runCleanup,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

// ============================================================
// Schema
// ============================================================

const spacetimedb = schema({
  user,
  room,
  roomMember,
  message,
  messageEdit,
  typingIndicator,
  readReceipt,
  userRoomRead,
  reaction,
  scheduledMessage,
  cleanupTimer,
});

export default spacetimedb;

// ============================================================
// Lifecycle Hooks
// ============================================================

export const init = spacetimedb.init((ctx) => {
  // Start cleanup timer (repeating every 10 seconds)
  ctx.db.cleanupTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(10_000_000n),
  });
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true, lastActive: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActive: ctx.timestamp });
  }
  // Remove this user's typing indicators
  for (const ind of [...ctx.db.typingIndicator.iter()]) {
    if (ind.userId.equals(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(ind.id);
    }
  }
});

// ============================================================
// Reducers — User Management
// ============================================================

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.user.identity.find(ctx.sender)) {
      throw new SenderError('already registered');
    }
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 32) {
      throw new SenderError('name must be 1-32 characters');
    }
    ctx.db.user.insert({
      identity: ctx.sender,
      name: trimmed,
      online: true,
      status: 'online',
      lastActive: ctx.timestamp,
    });
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const validStatuses = ['online', 'away', 'dnd', 'invisible'];
    if (!validStatuses.includes(status)) {
      throw new SenderError('invalid status');
    }
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('not registered');
    ctx.db.user.identity.update({ ...u, status, lastActive: ctx.timestamp });
  }
);

export const updateActivity = spacetimedb.reducer((ctx) => {
  const u = ctx.db.user.identity.find(ctx.sender);
  if (!u) return;
  ctx.db.user.identity.update({ ...u, lastActive: ctx.timestamp });
});

// ============================================================
// Reducers — Room Management
// ============================================================

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 64) {
      throw new SenderError('room name must be 1-64 characters');
    }
    const newRoom = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: newRoom.id,
      identity: ctx.sender,
      isAdmin: true,
      isBanned: false,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('room not found');
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) {
        if (m.isBanned) throw new SenderError('you are banned from this room');
        return; // already a member
      }
    }
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      isAdmin: false,
      isBanned: false,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
    throw new SenderError('not a member of this room');
  }
);

// ============================================================
// Reducers — Messages
// ============================================================

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function getMember(ctx: any, roomId: bigint, identity: any) {
  for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
    if (m.identity.equals(identity)) return m;
  }
  return null;
}

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member) throw new SenderError('not a member of this room');
    if (member.isBanned) throw new SenderError('you are banned from this room');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('invalid message');
    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: null,
      isEphemeral: false,
      expiresAt: null,
      isDeleted: false,
    });
    // Clear typing indicator for this user in this room
    for (const ind of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ind.userId.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ind.id);
        break;
      }
    }
    // Update activity
    const u = ctx.db.user.identity.find(ctx.sender);
    if (u) ctx.db.user.identity.update({ ...u, lastActive: ctx.timestamp });
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSeconds: t.u32() },
  (ctx, { roomId, text, durationSeconds }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member) throw new SenderError('not a member of this room');
    if (member.isBanned) throw new SenderError('you are banned from this room');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('invalid message');
    if (durationSeconds < 1 || durationSeconds > 3600) throw new SenderError('duration must be 1-3600 seconds');
    const expiresAt = {
      microsSinceUnixEpoch: ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSeconds) * 1_000_000n,
    };
    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: null,
      isEphemeral: true,
      expiresAt,
      isDeleted: false,
    });
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('not your message');
    if (msg.isDeleted) throw new SenderError('message has been deleted');
    const trimmed = newText.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('invalid text');
    // Save current version to edit history
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      text: msg.text,
      editedAt: ctx.timestamp,
    });
    // Update the message
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// ============================================================
// Reducers — Scheduled Messages
// ============================================================

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member) throw new SenderError('not a member of this room');
    if (member.isBanned) throw new SenderError('you are banned');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('invalid message');
    if (sendAtMicros <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('scheduled time must be in the future');
    }
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
      roomId,
      sender: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const msg = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!msg) throw new SenderError('scheduled message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('not your scheduled message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// Called automatically by SpacetimeDB when a scheduled message fires
export const sendScheduledMessage = spacetimedb.reducer(
  { scheduled: scheduledMessage.rowType },
  (ctx, { scheduled }) => {
    ctx.db.message.insert({
      id: 0n,
      roomId: scheduled.roomId,
      sender: scheduled.sender,
      text: scheduled.text,
      sentAt: ctx.timestamp,
      editedAt: null,
      isEphemeral: false,
      expiresAt: null,
      isDeleted: false,
    });
    // scheduled row is auto-deleted after this reducer runs
  }
);

// ============================================================
// Reducers — Typing Indicators
// ============================================================

export const updateTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    // Find existing indicator for this user in this room
    let existing = null;
    for (const ind of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ind.userId.equals(ctx.sender)) {
        existing = ind;
        break;
      }
    }
    if (!isTyping) {
      if (existing) ctx.db.typingIndicator.id.delete(existing.id);
      return;
    }
    const expiresAt = {
      microsSinceUnixEpoch: ctx.timestamp.microsSinceUnixEpoch + 5_000_000n,
    };
    if (existing) {
      ctx.db.typingIndicator.id.update({ ...existing, expiresAt });
    } else {
      ctx.db.typingIndicator.insert({
        id: 0n,
        roomId,
        userId: ctx.sender,
        expiresAt,
      });
    }
  }
);

// ============================================================
// Reducers — Read Receipts & Unread Counts
// ============================================================

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    // Update or insert userRoomRead
    let existingRead = null;
    for (const r of [...ctx.db.userRoomRead.userId.filter(ctx.sender)]) {
      if (r.roomId === roomId) {
        existingRead = r;
        break;
      }
    }
    if (existingRead) {
      if (messageId > existingRead.lastReadMessageId) {
        ctx.db.userRoomRead.id.update({ ...existingRead, lastReadMessageId: messageId });
      }
    } else {
      ctx.db.userRoomRead.insert({
        id: 0n,
        userId: ctx.sender,
        roomId,
        lastReadMessageId: messageId,
      });
    }
    // Add read receipt for this message if not already present
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) return;
    for (const r of [...ctx.db.readReceipt.messageId.filter(messageId)]) {
      if (r.userId.equals(ctx.sender)) return; // already receipted
    }
    ctx.db.readReceipt.insert({
      id: 0n,
      messageId,
      userId: ctx.sender,
      seenAt: ctx.timestamp,
    });
  }
);

// ============================================================
// Reducers — Reactions
// ============================================================

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('message not found');
    if (msg.isDeleted) throw new SenderError('message has been deleted');
    // Toggle: remove if already reacted with this emoji, otherwise add
    for (const r of [...ctx.db.reaction.messageId.filter(messageId)]) {
      if (r.userId.equals(ctx.sender) && r.emoji === emoji) {
        ctx.db.reaction.id.delete(r.id);
        return;
      }
    }
    ctx.db.reaction.insert({
      id: 0n,
      messageId,
      userId: ctx.sender,
      emoji,
    });
  }
);

// ============================================================
// Reducers — Permissions (Kick/Ban/Promote)
// ============================================================

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    let callerMember = null;
    let targetMember = null;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) callerMember = m;
      if (m.identity.equals(targetIdentity)) targetMember = m;
    }
    if (!callerMember || !callerMember.isAdmin) throw new SenderError('not an admin');
    if (!targetMember) throw new SenderError('target user not in room');
    if (targetMember.isAdmin && !callerMember.identity.equals(ctx.db.room.id.find(roomId)?.createdBy)) {
      throw new SenderError('cannot kick another admin');
    }
    ctx.db.roomMember.id.delete(targetMember.id);
  }
);

export const banUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    let callerMember = null;
    let targetMember = null;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) callerMember = m;
      if (m.identity.equals(targetIdentity)) targetMember = m;
    }
    if (!callerMember || !callerMember.isAdmin) throw new SenderError('not an admin');
    if (!targetMember) throw new SenderError('target user not in room');
    ctx.db.roomMember.id.update({ ...targetMember, isBanned: true });
  }
);

export const promoteToAdmin = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    let callerMember = null;
    let targetMember = null;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) callerMember = m;
      if (m.identity.equals(targetIdentity)) targetMember = m;
    }
    if (!callerMember || !callerMember.isAdmin) throw new SenderError('not an admin');
    if (!targetMember) throw new SenderError('target user not in room');
    ctx.db.roomMember.id.update({ ...targetMember, isAdmin: true });
  }
);

// ============================================================
// Scheduled Reducer — Cleanup (typing indicators + ephemeral messages + auto-away)
// ============================================================

export const runCleanup = spacetimedb.reducer(
  { scheduled: cleanupTimer.rowType },
  (ctx, _args) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;

    // Delete expired typing indicators
    for (const ind of [...ctx.db.typingIndicator.iter()]) {
      if (ind.expiresAt.microsSinceUnixEpoch <= now) {
        ctx.db.typingIndicator.id.delete(ind.id);
      }
    }

    // Delete expired ephemeral messages
    for (const msg of [...ctx.db.message.iter()]) {
      if (msg.isEphemeral && msg.expiresAt !== null && msg.expiresAt.microsSinceUnixEpoch <= now) {
        ctx.db.message.id.delete(msg.id);
      }
    }

    // Auto-set users to 'away' after 5 minutes of inactivity
    const fiveMinutes = 300_000_000n; // 5 * 60 * 1_000_000 microseconds
    for (const u of [...ctx.db.user.iter()]) {
      if (u.online && u.status === 'online') {
        if (now - u.lastActive.microsSinceUnixEpoch > fiveMinutes) {
          ctx.db.user.identity.update({ ...u, status: 'away' });
        }
      }
    }
  }
);
