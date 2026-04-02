import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  user,
  room,
  roomMember,
  message,
  messageEdit,
  typingIndicator,
  readReceipt,
  roomLastRead,
  messageReaction,
} from './schema';

// Scheduled tables — use string names to reference their reducers (no circular deps)
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: 'send_scheduled_message',
    indexes: [
      { name: 'by_sender', algorithm: 'btree' as const, columns: ['senderIdentity'] as const },
    ],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    text: t.string(),
  }
);

const ephemeralCleanup = table(
  {
    name: 'ephemeral_cleanup',
    scheduled: 'delete_ephemeral_message',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

const typingCleanup = table(
  {
    name: 'typing_cleanup',
    scheduled: 'expire_typing_indicator',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    typingIndicatorId: t.u64(),
  }
);

const awayCleanup = table(
  {
    name: 'away_cleanup',
    scheduled: 'check_away_status',
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    targetIdentity: t.identity(),
  }
);

const spacetimedb = schema(
  user,
  room,
  roomMember,
  message,
  messageEdit,
  typingIndicator,
  readReceipt,
  roomLastRead,
  messageReaction,
  scheduledMessage,
  ephemeralCleanup,
  typingCleanup,
  awayCleanup
);
export default spacetimedb;

// ============ LIFECYCLE HOOKS ============

spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing && existing.status !== 'invisible') {
    ctx.db.user.identity.update({ ...existing, status: 'online', lastActive: ctx.timestamp });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, lastActive: ctx.timestamp });
    // Remove typing indicators for this user across all rooms
    for (const ti of [...ctx.db.typingIndicator.iter()]) {
      if (ti.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
});

// ============ USER REDUCERS ============

export const register = spacetimedb.reducer(
  'register',
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (!trimmed) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Already registered');
    ctx.db.user.insert({
      identity: ctx.sender,
      name: trimmed,
      status: 'online',
      lastActive: ctx.timestamp,
    });
  }
);

export const updateStatus = spacetimedb.reducer(
  'update_status',
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, status, lastActive: ctx.timestamp });
  }
);

export const updateActivity = spacetimedb.reducer(
  'update_activity',
  (ctx) => {
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) return;
    const updated: typeof existing = { ...existing, lastActive: ctx.timestamp };
    if (existing.status === 'away') updated.status = 'online';
    ctx.db.user.identity.update(updated);
  }
);

// ============ ROOM REDUCERS ============

export const createRoom = spacetimedb.reducer(
  'create_room',
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (!trimmed) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 chars)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
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
      role: 'admin',
      banned: false,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  'join_room',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    for (const m of [...ctx.db.roomMember.by_room_id.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) {
        if (m.banned) throw new SenderError('You are banned from this room');
        return; // Already a member
      }
    }
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      role: 'member',
      banned: false,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  'leave_room',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of [...ctx.db.roomMember.by_room_id.filter(roomId)]) {
      if (m.identity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
  }
);

// ============ PERMISSION REDUCERS ============

function findMember(ctx: any, roomId: bigint, identity: any) {
  for (const m of [...ctx.db.roomMember.by_room_id.filter(roomId)]) {
    if (m.identity.equals(identity)) return m;
  }
  return null;
}

function assertMember(ctx: any, roomId: bigint) {
  const m = findMember(ctx, roomId, ctx.sender);
  if (!m || m.banned) throw new SenderError('Not a member of this room');
  return m;
}

export const kickUser = spacetimedb.reducer(
  'kick_user',
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const admin = findMember(ctx, roomId, ctx.sender);
    if (!admin || admin.role !== 'admin') throw new SenderError('Not an admin');
    const target = findMember(ctx, roomId, targetIdentity);
    if (!target) throw new SenderError('User not in room');
    ctx.db.roomMember.id.update({ ...target, banned: true });
  }
);

export const promoteUser = spacetimedb.reducer(
  'promote_user',
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const admin = findMember(ctx, roomId, ctx.sender);
    if (!admin || admin.role !== 'admin') throw new SenderError('Not an admin');
    const target = findMember(ctx, roomId, targetIdentity);
    if (!target) throw new SenderError('User not in room');
    ctx.db.roomMember.id.update({ ...target, role: 'admin' });
  }
);

export const unbanUser = spacetimedb.reducer(
  'unban_user',
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const admin = findMember(ctx, roomId, ctx.sender);
    if (!admin || admin.role !== 'admin') throw new SenderError('Not an admin');
    const target = findMember(ctx, roomId, targetIdentity);
    if (!target) throw new SenderError('User not found');
    ctx.db.roomMember.id.update({ ...target, banned: false });
  }
);

// ============ MESSAGE REDUCERS ============

function markRoomRead(ctx: any, roomId: bigint, messageId: bigint) {
  for (const lr of [...ctx.db.roomLastRead.by_room_id.filter(roomId)]) {
    if (lr.identity.equals(ctx.sender)) {
      if (messageId > lr.lastReadMessageId) {
        ctx.db.roomLastRead.id.update({ ...lr, lastReadMessageId: messageId });
      }
      return;
    }
  }
  ctx.db.roomLastRead.insert({
    id: 0n,
    roomId,
    identity: ctx.sender,
    lastReadMessageId: messageId,
  });
}

export const sendMessage = spacetimedb.reducer(
  'send_message',
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    assertMember(ctx, roomId);
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: false,
      expiresAt: undefined,
      deleted: false,
    });
    markRoomRead(ctx, roomId, msg.id);
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  'send_ephemeral_message',
  { roomId: t.u64(), text: t.string(), durationSeconds: t.u32() },
  (ctx, { roomId, text, durationSeconds }) => {
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (durationSeconds < 10 || durationSeconds > 3600) {
      throw new SenderError('Duration must be 10-3600 seconds');
    }
    assertMember(ctx, roomId);
    const expiresMicros =
      ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSeconds) * 1_000_000n;
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: true,
      expiresAt: new Timestamp(expiresMicros),
      deleted: false,
    });
    ctx.db.ephemeralCleanup.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresMicros),
      messageId: msg.id,
    });
    markRoomRead(ctx, roomId, msg.id);
  }
);

export const editMessage = spacetimedb.reducer(
  'edit_message',
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Cannot edit others messages');
    if (msg.deleted) throw new SenderError('Cannot edit deleted message');
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      oldText: msg.text,
      editedAt: ctx.timestamp,
    });
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

export const deleteMessage = spacetimedb.reducer(
  'delete_message',
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) {
      const m = findMember(ctx, msg.roomId, ctx.sender);
      if (!m || m.role !== 'admin') throw new SenderError('Cannot delete others messages');
    }
    ctx.db.message.id.update({ ...msg, deleted: true, text: '[deleted]' });
  }
);

// ============ TYPING INDICATORS ============

export const setTyping = spacetimedb.reducer(
  'set_typing',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    assertMember(ctx, roomId);
    const expiresMicros = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
    let existing: any = null;
    for (const ti of [...ctx.db.typingIndicator.by_room_id.filter(roomId)]) {
      if (ti.identity.equals(ctx.sender)) {
        existing = ti;
        break;
      }
    }
    if (existing) {
      ctx.db.typingIndicator.id.update({ ...existing, lastTypedAt: ctx.timestamp });
      ctx.db.typingCleanup.insert({
        scheduledId: 0n,
        scheduledAt: ScheduleAt.time(expiresMicros),
        typingIndicatorId: existing.id,
      });
    } else {
      const ti = ctx.db.typingIndicator.insert({
        id: 0n,
        roomId,
        identity: ctx.sender,
        lastTypedAt: ctx.timestamp,
      });
      ctx.db.typingCleanup.insert({
        scheduledId: 0n,
        scheduledAt: ScheduleAt.time(expiresMicros),
        typingIndicatorId: ti.id,
      });
    }
  }
);

export const clearTyping = spacetimedb.reducer(
  'clear_typing',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const ti of [...ctx.db.typingIndicator.by_room_id.filter(roomId)]) {
      if (ti.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ti.id);
        break;
      }
    }
  }
);

export const expireTypingIndicator = spacetimedb.reducer(
  'expire_typing_indicator',
  { timer: typingCleanup.rowType },
  (ctx, { timer }) => {
    const ti = ctx.db.typingIndicator.id.find(timer.typingIndicatorId);
    if (!ti) return;
    const expiresAt = ti.lastTypedAt.microsSinceUnixEpoch + 5_000_000n;
    if (ctx.timestamp.microsSinceUnixEpoch >= expiresAt) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
);

// ============ READ RECEIPTS ============

export const markMessageRead = spacetimedb.reducer(
  'mark_message_read',
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) return;
    assertMember(ctx, msg.roomId);
    let alreadySeen = false;
    for (const rr of [...ctx.db.readReceipt.by_message_id.filter(messageId)]) {
      if (rr.reader.equals(ctx.sender)) {
        alreadySeen = true;
        break;
      }
    }
    if (!alreadySeen) {
      ctx.db.readReceipt.insert({
        id: 0n,
        messageId,
        reader: ctx.sender,
        seenAt: ctx.timestamp,
      });
    }
    markRoomRead(ctx, msg.roomId, messageId);
  }
);

export const markRoomReadReducer = spacetimedb.reducer(
  'mark_room_read',
  { roomId: t.u64(), lastMessageId: t.u64() },
  (ctx, { roomId, lastMessageId }) => {
    assertMember(ctx, roomId);
    markRoomRead(ctx, roomId, lastMessageId);
  }
);

// ============ REACTIONS ============

export const toggleReaction = spacetimedb.reducer(
  'toggle_reaction',
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    assertMember(ctx, msg.roomId);
    for (const r of [...ctx.db.messageReaction.by_message_id.filter(messageId)]) {
      if (r.reactor.equals(ctx.sender) && r.emoji === emoji) {
        ctx.db.messageReaction.id.delete(r.id);
        return;
      }
    }
    ctx.db.messageReaction.insert({
      id: 0n,
      messageId,
      reactor: ctx.sender,
      emoji,
    });
  }
);

// ============ SCHEDULED MESSAGES ============

export const scheduleMessage = spacetimedb.reducer(
  'schedule_message',
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.i64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    assertMember(ctx, roomId);
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(BigInt(sendAtMicros)),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  'cancel_scheduled_message',
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const sm = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!sm) throw new SenderError('Scheduled message not found');
    if (!sm.senderIdentity.equals(ctx.sender)) {
      throw new SenderError('Cannot cancel others scheduled messages');
    }
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

export const sendScheduledMessage = spacetimedb.reducer(
  'send_scheduled_message',
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      sender: timer.senderIdentity,
      text: timer.text,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: false,
      expiresAt: undefined,
      deleted: false,
    });
    markRoomRead(ctx, timer.roomId, msg.id);
  }
);

// ============ EPHEMERAL CLEANUP ============

export const deleteEphemeralMessage = spacetimedb.reducer(
  'delete_ephemeral_message',
  { timer: ephemeralCleanup.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.id.find(timer.messageId);
    if (msg && msg.isEphemeral && !msg.deleted) {
      ctx.db.message.id.update({ ...msg, deleted: true, text: '[expired]' });
    }
  }
);

// ============ RICH USER PRESENCE ============

export const checkAwayStatus = spacetimedb.reducer(
  'check_away_status',
  { timer: awayCleanup.rowType },
  (ctx, { timer }) => {
    const u = ctx.db.user.identity.find(timer.targetIdentity);
    if (!u || u.status !== 'online') return;
    const inactiveMicros =
      ctx.timestamp.microsSinceUnixEpoch - u.lastActive.microsSinceUnixEpoch;
    if (inactiveMicros >= 300_000_000n) {
      ctx.db.user.identity.update({ ...u, status: 'away' });
    }
  }
);

export const setAwayTimer = spacetimedb.reducer(
  'set_away_timer',
  (ctx) => {
    const u = ctx.db.user.identity.find(ctx.sender);
    if (!u) return;
    ctx.db.awayCleanup.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 300_000_000n),
      targetIdentity: ctx.sender,
    });
  }
);
