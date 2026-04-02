import { schema as makeSchema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  user, room, roomMember, message, messageEdit,
  reaction, readReceipt, typingIndicator, scheduledMessage
} from './schema';

// Timer table — defined here so processMessageTimer is in scope for the circular ref
const messageTimer = table(
  {
    name: 'message_timer',
    scheduled: (): any => processMessageTimer,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
    timerType: t.string(), // "ephemeral_delete" | "scheduled_send"
  }
);

const spacetimedb = makeSchema({
  user,
  room,
  roomMember,
  message,
  messageEdit,
  reaction,
  readReceipt,
  typingIndicator,
  scheduledMessage,
  messageTimer,
});

export default spacetimedb;

// =========================================
// Lifecycle Hooks
// =========================================

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
  // Clean up typing indicators on disconnect
  for (const indicator of [...ctx.db.typingIndicator.iter()]) {
    if (indicator.identity.equals(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }
});

// =========================================
// User Management
// =========================================

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 30) {
      throw new SenderError('Name must be 1-30 characters');
    }
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, name: trimmed, online: true, lastActive: ctx.timestamp });
      return;
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
      throw new SenderError('Invalid status');
    }
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, status, lastActive: ctx.timestamp });
  }
);

export const updateActivity = spacetimedb.reducer(
  {},
  (ctx, _args) => {
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) return;
    const newStatus = existing.status === 'away' ? 'online' : existing.status;
    ctx.db.user.identity.update({ ...existing, lastActive: ctx.timestamp, status: newStatus });
  }
);

// =========================================
// Room Management
// =========================================

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (!userRow) throw new SenderError('Not registered');
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 50) {
      throw new SenderError('Room name must be 1-50 characters');
    }
    const newRoom = ctx.db.room.insert({ id: 0n, name: trimmed, creatorIdentity: ctx.sender });
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
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (!userRow) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.identity.equals(ctx.sender)) {
        if (member.isBanned) throw new SenderError('You are banned from this room');
        return;
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
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.identity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(member.id);
        return;
      }
    }
  }
);

// =========================================
// Admin / Permissions
// =========================================

function getMember(ctx: any, roomId: bigint, identity: any) {
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.identity.equals(identity)) return member;
  }
  return null;
}

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const adminMember = getMember(ctx, roomId, ctx.sender);
    if (!adminMember || !adminMember.isAdmin) throw new SenderError('Not admin');
    const target = getMember(ctx, roomId, targetIdentity);
    if (!target) throw new SenderError('Target not in room');
    ctx.db.roomMember.id.update({ ...target, isBanned: true });
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const adminMember = getMember(ctx, roomId, ctx.sender);
    if (!adminMember || !adminMember.isAdmin) throw new SenderError('Not admin');
    const target = getMember(ctx, roomId, targetIdentity);
    if (!target) throw new SenderError('Target not in room');
    ctx.db.roomMember.id.update({ ...target, isAdmin: true });
  }
);

// =========================================
// Messaging
// =========================================

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (!userRow) throw new SenderError('Not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member || member.isBanned) throw new SenderError('Not a member of this room');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 characters');
    ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: false,
      ephemeralDurationSeconds: 0,
      isDeleted: false,
    });
    // Clear typing indicator
    for (const ind of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (ind.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ind.id);
        break;
      }
    }
    ctx.db.user.identity.update({ ...userRow, lastActive: ctx.timestamp });
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSeconds: t.u32() },
  (ctx, { roomId, text, durationSeconds }) => {
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (!userRow) throw new SenderError('Not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member || member.isBanned) throw new SenderError('Not a member of this room');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 characters');
    const validDurations = [60, 300, 3600];
    const dur = validDurations.includes(durationSeconds) ? durationSeconds : 60;
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: true,
      ephemeralDurationSeconds: dur,
      isDeleted: false,
    });
    const delayMicros = BigInt(dur) * 1_000_000n;
    ctx.db.messageTimer.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delayMicros),
      messageId: msg.id,
      timerType: 'ephemeral_delete',
    });
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.senderIdentity.equals(ctx.sender)) throw new SenderError('Not your message');
    if (msg.isDeleted) throw new SenderError('Message is deleted');
    const trimmed = newText.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 characters');
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId: msg.id,
      previousText: msg.text,
      editedAt: ctx.timestamp,
    });
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// =========================================
// Reactions
// =========================================

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    const member = getMember(ctx, msg.roomId, ctx.sender);
    if (!member || member.isBanned) throw new SenderError('Not a member');
    for (const r of ctx.db.reaction.by_message.filter(messageId)) {
      if (r.userIdentity.equals(ctx.sender) && r.emoji === emoji) {
        ctx.db.reaction.id.delete(r.id);
        return;
      }
    }
    ctx.db.reaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
  }
);

// =========================================
// Read Receipts
// =========================================

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    for (const receipt of ctx.db.readReceipt.by_room.filter(roomId)) {
      if (receipt.identity.equals(ctx.sender)) {
        if (receipt.lastReadMessageId < messageId) {
          ctx.db.readReceipt.id.update({ ...receipt, lastReadMessageId: messageId, readAt: ctx.timestamp });
        }
        return;
      }
    }
    ctx.db.readReceipt.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      lastReadMessageId: messageId,
      readAt: ctx.timestamp,
    });
  }
);

// =========================================
// Typing Indicators
// =========================================

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member || member.isBanned) throw new SenderError('Not a member');
    for (const ind of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (ind.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.update({ ...ind, expiresAt: ctx.timestamp });
        return;
      }
    }
    ctx.db.typingIndicator.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      expiresAt: ctx.timestamp,
    });
  }
);

export const clearTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const ind of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (ind.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ind.id);
        return;
      }
    }
  }
);

// =========================================
// Scheduled Messages
// =========================================

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.i64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const member = getMember(ctx, roomId, ctx.sender);
    if (!member || member.isBanned) throw new SenderError('Not a member');
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Invalid message text');
    const sendAt = { microsSinceUnixEpoch: BigInt(sendAtMicros) } as any;
    const scheduled = ctx.db.scheduledMessage.insert({
      id: 0n,
      roomId,
      authorIdentity: ctx.sender,
      text: trimmed,
      sendAt,
      cancelled: false,
    });
    ctx.db.messageTimer.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(BigInt(sendAtMicros)),
      messageId: scheduled.id,
      timerType: 'scheduled_send',
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledMessageId: t.u64() },
  (ctx, { scheduledMessageId }) => {
    const scheduled = ctx.db.scheduledMessage.id.find(scheduledMessageId);
    if (!scheduled) throw new SenderError('Scheduled message not found');
    if (!scheduled.authorIdentity.equals(ctx.sender)) throw new SenderError('Not your message');
    ctx.db.scheduledMessage.id.update({ ...scheduled, cancelled: true });
  }
);

// =========================================
// Timer Processor (scheduled reducer)
// =========================================

export const processMessageTimer = spacetimedb.reducer(
  { timer: messageTimer.rowType },
  (ctx, { timer }) => {
    if (timer.timerType === 'ephemeral_delete') {
      const msg = ctx.db.message.id.find(timer.messageId);
      if (msg && !msg.isDeleted) {
        ctx.db.message.id.update({ ...msg, isDeleted: true, text: '[deleted]' });
      }
    } else if (timer.timerType === 'scheduled_send') {
      const scheduled = ctx.db.scheduledMessage.id.find(timer.messageId);
      if (scheduled && !scheduled.cancelled) {
        ctx.db.message.insert({
          id: 0n,
          roomId: scheduled.roomId,
          senderIdentity: scheduled.authorIdentity,
          text: scheduled.text,
          sentAt: ctx.timestamp,
          editedAt: undefined,
          isEphemeral: false,
          ephemeralDurationSeconds: 0,
          isDeleted: false,
        });
      }
    }
  }
);
