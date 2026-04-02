import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  user,
  room,
  roomMember,
  message,
  typingIndicator,
  messageRead,
  roomReadPosition,
  messageReaction,
  messageEditHistory,
} from './schema';

// ── Scheduled Tables (defined here to co-locate with their reducers) ───────

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

const ephemeralDeleteTimer = table(
  {
    name: 'ephemeral_delete_timer',
    scheduled: (): any => deleteEphemeralMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

const typingCleanupTimer = table(
  {
    name: 'typing_cleanup_timer',
    scheduled: (): any => cleanupTypingIndicators,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const activityCheckTimer = table(
  {
    name: 'activity_check_timer',
    scheduled: (): any => checkUserActivity,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

// ── Schema Export ──────────────────────────────────────────────────────────

const spacetimedb = schema({
  user,
  room,
  roomMember,
  message,
  typingIndicator,
  messageRead,
  roomReadPosition,
  messageReaction,
  messageEditHistory,
  scheduledMessage,
  ephemeralDeleteTimer,
  typingCleanupTimer,
  activityCheckTimer,
});

export default spacetimedb;

// ── Lifecycle Hooks ────────────────────────────────────────────────────────

export const init = spacetimedb.init((ctx) => {
  // Start repeating timers
  ctx.db.typingCleanupTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(8_000_000n), // 8 seconds
  });
  ctx.db.activityCheckTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(60_000_000n), // 60 seconds
  });
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const u = ctx.db.user.identity.find(ctx.sender);
  if (u) {
    const newStatus = u.status === 'invisible' ? 'invisible' : 'online';
    ctx.db.user.identity.update({ ...u, online: true, status: newStatus, lastActive: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const u = ctx.db.user.identity.find(ctx.sender);
  if (u) {
    ctx.db.user.identity.update({ ...u, online: false, lastActive: ctx.timestamp });
  }
  // Clear all typing indicators for this user
  for (const ind of ctx.db.typingIndicator.identity.filter(ctx.sender)) {
    ctx.db.typingIndicator.id.delete(ind.id);
  }
});

// ── User Reducers ──────────────────────────────────────────────────────────

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32)');

    // Check name uniqueness
    for (const u of ctx.db.user.iter()) {
      if (u.name.toLowerCase() === trimmed.toLowerCase() && !u.identity.equals(ctx.sender)) {
        throw new SenderError('Name already taken');
      }
    }

    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, name: trimmed, lastActive: ctx.timestamp });
    } else {
      ctx.db.user.insert({
        identity: ctx.sender,
        name: trimmed,
        online: true,
        status: 'online',
        lastActive: ctx.timestamp,
      });
    }
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...u, status, lastActive: ctx.timestamp });
  }
);

export const updateActivity = spacetimedb.reducer((ctx) => {
  const u = ctx.db.user.identity.find(ctx.sender);
  if (!u) return;
  const newStatus = u.status === 'away' ? 'online' : u.status;
  ctx.db.user.identity.update({ ...u, lastActive: ctx.timestamp, status: newStatus });
});

// ── Room Reducers ──────────────────────────────────────────────────────────

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('Not registered');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64)');

    const r = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });

    // Creator joins as admin
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: r.id,
      identity: ctx.sender,
      isAdmin: true,
      isBanned: false,
    });

    // Initialize read position for creator
    ctx.db.roomReadPosition.insert({
      id: 0n,
      roomId: r.id,
      identity: ctx.sender,
      lastReadMessageId: 0n,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(ctx.sender)) {
        if (m.isBanned) throw new SenderError('You are banned from this room');
        return; // Already a member
      }
    }

    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      isAdmin: false,
      isBanned: false,
    });

    // Init read position to most recent message
    const msgs = [...ctx.db.message.roomId.filter(roomId)];
    const lastId = msgs.length > 0 ? msgs.sort((a, b) => (a.id < b.id ? -1 : 1))[msgs.length - 1].id : 0n;
    ctx.db.roomReadPosition.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      lastReadMessageId: lastId,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
  }
);

// ── Message Reducers ───────────────────────────────────────────────────────

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), isEphemeral: t.bool(), ephemeralDurationSecs: t.u64() },
  (ctx, { roomId, text, isEphemeral, ephemeralDurationSecs }) => {
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('Not registered');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 4000) throw new SenderError('Message too long');

    // Verify membership and not banned
    let isMember = false;
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(ctx.sender)) {
        if (m.isBanned) throw new SenderError('You are banned from this room');
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      isEphemeral,
      ephemeralDurationSecs: isEphemeral ? ephemeralDurationSecs : 0n,
      isEdited: false,
      isDeleted: false,
    });

    // Schedule ephemeral deletion
    if (isEphemeral && ephemeralDurationSecs > 0n) {
      const delayMicros = ephemeralDurationSecs * 1_000_000n;
      ctx.db.ephemeralDeleteTimer.insert({
        scheduledId: 0n,
        scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros),
        messageId: msg.id,
      });
    }

    // Update sender's read position
    _updateReadPosition(ctx, roomId, msg.id);

    // Clear sender's typing indicator
    for (const ind of ctx.db.typingIndicator.roomId.filter(roomId)) {
      if (ind.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ind.id);
        break;
      }
    }

    // Update lastActive
    ctx.db.user.identity.update({ ...u, lastActive: ctx.timestamp });
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    if (msg.isDeleted) throw new SenderError('Cannot edit a deleted message');
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 4000) throw new SenderError('Message too long');

    // Save original to history
    ctx.db.messageEditHistory.insert({
      id: 0n,
      messageId,
      text: msg.text,
      editedAt: ctx.timestamp,
    });

    ctx.db.message.id.update({ ...msg, text: trimmed, isEdited: true });
  }
);

// ── Typing Indicators ──────────────────────────────────────────────────────

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;

    // Find existing indicator
    let existing: { id: bigint; roomId: bigint; identity: any; expiresAtMicros: bigint } | null = null;
    for (const ind of ctx.db.typingIndicator.roomId.filter(roomId)) {
      if (ind.identity.equals(ctx.sender)) {
        existing = ind;
        break;
      }
    }

    if (isTyping) {
      const expiresAt = ctx.timestamp.microsSinceUnixEpoch + 6_000_000n; // 6 seconds
      if (existing) {
        ctx.db.typingIndicator.id.update({ ...existing, expiresAtMicros: expiresAt });
      } else {
        ctx.db.typingIndicator.insert({
          id: 0n,
          roomId,
          identity: ctx.sender,
          expiresAtMicros: expiresAt,
        });
      }
    } else {
      if (existing) ctx.db.typingIndicator.id.delete(existing.id);
    }
  }
);

// ── Read Receipts & Unread ─────────────────────────────────────────────────

export const markMessageRead = spacetimedb.reducer(
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    // Check if already marked
    for (const r of ctx.db.messageRead.messageId.filter(messageId)) {
      if (r.identity.equals(ctx.sender)) return;
    }
    ctx.db.messageRead.insert({ id: 0n, messageId, identity: ctx.sender });

    // Update read position
    const msg = ctx.db.message.id.find(messageId);
    if (msg) _updateReadPosition(ctx, msg.roomId, messageId);
  }
);

export const markRoomRead = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    // Find the latest message in the room
    const msgs = [...ctx.db.message.roomId.filter(roomId)].filter(m => !m.isDeleted);
    if (msgs.length === 0) return;
    const latestMsg = msgs.sort((a, b) => (a.id < b.id ? -1 : 1))[msgs.length - 1];

    _updateReadPosition(ctx, roomId, latestMsg.id);

    // Mark all messages as read
    for (const msg of msgs) {
      let alreadyRead = false;
      for (const r of ctx.db.messageRead.messageId.filter(msg.id)) {
        if (r.identity.equals(ctx.sender)) { alreadyRead = true; break; }
      }
      if (!alreadyRead) {
        ctx.db.messageRead.insert({ id: 0n, messageId: msg.id, identity: ctx.sender });
      }
    }
  }
);

// ── Message Reactions ──────────────────────────────────────────────────────

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    // Toggle: remove if exists, add if not
    for (const r of ctx.db.messageReaction.messageId.filter(messageId)) {
      if (r.identity.equals(ctx.sender) && r.emoji === emoji) {
        ctx.db.messageReaction.id.delete(r.id);
        return;
      }
    }
    ctx.db.messageReaction.insert({ id: 0n, messageId, identity: ctx.sender, emoji });
  }
);

// ── Scheduled Messages ─────────────────────────────────────────────────────

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) throw new SenderError('Not registered');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');

    // Verify membership
    let isMember = false;
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(ctx.sender) && !m.isBanned) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    // Must be in the future (at least 5 seconds)
    const minTime = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
    if (sendAtMicros < minTime) throw new SenderError('Schedule time must be at least 5 seconds in the future');

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
    const sched = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!sched) throw new SenderError('Scheduled message not found');
    if (!sched.sender.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// ── Room Permissions ───────────────────────────────────────────────────────

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    _requireAdmin(ctx, roomId);

    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        if (m.identity.equals(ctx.sender)) throw new SenderError('Cannot kick yourself');
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

export const banUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    _requireAdmin(ctx, roomId);

    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        if (m.identity.equals(ctx.sender)) throw new SenderError('Cannot ban yourself');
        ctx.db.roomMember.id.update({ ...m, isBanned: true });
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    _requireAdmin(ctx, roomId);

    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        ctx.db.roomMember.id.update({ ...m, isAdmin: true });
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

// ── Scheduled Reducer Implementations ─────────────────────────────────────

export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Verify sender is still a non-banned member
    let isMember = false;
    for (const m of ctx.db.roomMember.roomId.filter(timer.roomId)) {
      if (m.identity.equals(timer.sender) && !m.isBanned) {
        isMember = true;
        break;
      }
    }
    if (!isMember) return; // Silently drop if no longer a member

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      sender: timer.sender,
      text: timer.text,
      sentAt: ctx.timestamp,
      isEphemeral: false,
      ephemeralDurationSecs: 0n,
      isEdited: false,
      isDeleted: false,
    });

    _updateReadPositionForIdentity(ctx, timer.roomId, msg.id, timer.sender);
  }
);

export const deleteEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralDeleteTimer.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.id.find(timer.messageId);
    if (msg && !msg.isDeleted) {
      ctx.db.message.id.update({ ...msg, isDeleted: true, text: '[Message expired]' });
    }
  }
);

export const cleanupTypingIndicators = spacetimedb.reducer(
  { timer: typingCleanupTimer.rowType },
  (ctx, { timer }) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const ind of ctx.db.typingIndicator.iter()) {
      if (ind.expiresAtMicros <= now) {
        ctx.db.typingIndicator.id.delete(ind.id);
      }
    }
  }
);

export const checkUserActivity = spacetimedb.reducer(
  { timer: activityCheckTimer.rowType },
  (ctx, { timer }) => {
    const fiveMinutesMicros = 5n * 60n * 1_000_000n;
    const threshold = ctx.timestamp.microsSinceUnixEpoch - fiveMinutesMicros;
    for (const u of ctx.db.user.iter()) {
      if (u.online && u.status === 'online' && u.lastActive.microsSinceUnixEpoch < threshold) {
        ctx.db.user.identity.update({ ...u, status: 'away' });
      }
    }
  }
);

// ── Internal Helpers ───────────────────────────────────────────────────────

function _requireAdmin(ctx: any, roomId: bigint): void {
  for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
    if (m.identity.equals(ctx.sender)) {
      if (!m.isAdmin) throw new SenderError('Not an admin of this room');
      return;
    }
  }
  throw new SenderError('Not a member of this room');
}

function _updateReadPosition(ctx: any, roomId: bigint, messageId: bigint): void {
  _updateReadPositionForIdentity(ctx, roomId, messageId, ctx.sender);
}

function _updateReadPositionForIdentity(ctx: any, roomId: bigint, messageId: bigint, identity: any): void {
  for (const pos of ctx.db.roomReadPosition.roomId.filter(roomId)) {
    if (pos.identity.equals(identity)) {
      if (pos.lastReadMessageId < messageId) {
        ctx.db.roomReadPosition.id.update({ ...pos, lastReadMessageId: messageId });
      }
      return;
    }
  }
  ctx.db.roomReadPosition.insert({
    id: 0n,
    roomId,
    identity,
    lastReadMessageId: messageId,
  });
}
