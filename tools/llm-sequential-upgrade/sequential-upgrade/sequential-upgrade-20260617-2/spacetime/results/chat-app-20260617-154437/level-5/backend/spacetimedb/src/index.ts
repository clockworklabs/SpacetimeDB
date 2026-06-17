import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import spacetimedb from './schema';

export { default } from './schema';
export { sendScheduledMessage, deleteExpiredMessage } from './schema';

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true });
  } else {
    ctx.db.user.insert({ identity: ctx.sender, name: '', online: true });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false });
  }
  for (const ti of [...ctx.db.typingIndicator.iter()]) {
    if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
});

export const setName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, name: trimmed });
  }
);

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Room name too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const duplicate = [...ctx.db.room.iter()].find(r => r.name === trimmed);
    if (duplicate) throw new SenderError('Room name already taken');
    const room = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp });
    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (alreadyMember) return;
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
      }
    }
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAtUs: undefined, editedAt: undefined });
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) return;
    const existing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userIdentity.toHexString() === ctx.sender.toHexString());
    if (existing) {
      ctx.db.typingIndicator.id.update({ ...existing, updatedAt: ctx.timestamp });
    } else {
      ctx.db.typingIndicator.insert({ id: 0n, roomId, userIdentity: ctx.sender, updatedAt: ctx.timestamp });
    }
  }
);

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    const existing = [...ctx.db.readReceipt.roomId.filter(roomId)]
      .find(r => r.userIdentity.toHexString() === ctx.sender.toHexString());
    if (existing) {
      if (messageId > existing.lastReadMessageId) {
        ctx.db.readReceipt.id.update({ ...existing, lastReadMessageId: messageId });
      }
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: messageId });
    }
  }
);

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtUs: t.u64() },
  (ctx, { roomId, text, scheduledAtUs }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    if (scheduledAtUs <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }
    ctx.db.scheduledMessage.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(scheduledAtUs),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const scheduled = ctx.db.scheduledMessage.scheduled_id.find(scheduledId);
    if (!scheduled) throw new SenderError('Scheduled message not found');
    if (scheduled.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Cannot cancel another user\'s scheduled message');
    }
    ctx.db.scheduledMessage.scheduled_id.delete(scheduledId);
  }
);

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');
    const existing = [...ctx.db.messageReaction.messageId.filter(messageId)]
      .find(r => r.userIdentity.toHexString() === ctx.sender.toHexString() && r.emoji === emoji);
    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (msg.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Cannot edit another user\'s message');
    }
    ctx.db.messageEdit.insert({ id: 0n, messageId, editedAt: ctx.timestamp, previousText: msg.text });
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), ttlSeconds: t.u32() },
  (ctx, { roomId, text, ttlSeconds }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const ttlUs = BigInt(ttlSeconds) * 1_000_000n;
    const expiresAtUs = ctx.timestamp.microsSinceUnixEpoch + ttlUs;
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAtUs,
      editedAt: undefined,
    });
    ctx.db.messageExpiry.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(expiresAtUs),
      messageId: msg.id,
    });
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);
