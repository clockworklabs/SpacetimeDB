import { SenderError, t } from 'spacetimedb/server';
import spacetimedb, { ScheduleAt } from './schema.js';
export { default, sendScheduledMessage, deleteExpiredMessage } from './schema.js';

const ALLOWED_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false });
  }
  // Clean up typing indicators on disconnect
  for (const indicator of [...ctx.db.typingIndicator.iter()]) {
    if (indicator.userIdentity.equals(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }
});

export const setName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');

    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, name: trimmed });
    } else {
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true });
    }
  }
);

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long');

    const room = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp });
    ctx.db.membership.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    const existing = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (existing.length > 0) return;

    ctx.db.membership.insert({ id: 0n, roomId, userIdentity: ctx.sender });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    for (const m of memberships) {
      ctx.db.membership.id.delete(m.id);
    }
    for (const indicator of [...ctx.db.typingIndicator.by_room.filter(roomId)]) {
      if (indicator.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAt: undefined, editedAt: undefined });

    // Clear typing indicator when message is sent
    for (const indicator of [...ctx.db.typingIndicator.by_room.filter(roomId)]) {
      if (indicator.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    const indicators = [...ctx.db.typingIndicator.by_room.filter(roomId)].filter(i => i.userIdentity.equals(ctx.sender));

    if (isTyping) {
      if (indicators.length > 0) {
        ctx.db.typingIndicator.id.update({ ...indicators[0], updatedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.insert({ id: 0n, roomId, userIdentity: ctx.sender, updatedAt: ctx.timestamp });
      }
    } else {
      for (const indicator of indicators) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), lastReadMessageId: t.u64() },
  (ctx, { roomId, lastReadMessageId }) => {
    const existing = [...ctx.db.readReceipt.by_room_user.filter([roomId, ctx.sender])];
    if (existing.length > 0) {
      ctx.db.readReceipt.id.update({ ...existing[0], lastReadMessageId });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId });
    }
  }
);

// Schedule a message to be sent at a future time (scheduledAtMicros = Unix epoch microseconds)
export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtMicros: t.i64() },
  (ctx, { roomId, text, scheduledAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    ctx.db.scheduledMessage.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(BigInt(scheduledAtMicros)),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

// Cancel a pending scheduled message (only the sender can cancel)
export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const row = ctx.db.scheduledMessage.scheduled_id.find(scheduledId);
    if (!row) throw new SenderError('Scheduled message not found');
    if (!row.senderIdentity.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.scheduled_id.delete(scheduledId);
  }
);

// Toggle a reaction emoji on a message (add if absent, remove if present)
export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    if (!ALLOWED_EMOJIS.includes(emoji)) throw new SenderError('Invalid emoji');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    const existing = [...ctx.db.messageReaction.by_message_user.filter([messageId, ctx.sender])]
      .find(r => r.emoji === emoji);

    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

// Edit a message (only the sender can edit); saves previous text as edit history
export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.senderIdentity.equals(ctx.sender)) throw new SenderError('Can only edit your own messages');

    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    // Save current text to edit history before overwriting
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      previousText: msg.text,
      editedAt: ctx.timestamp,
    });

    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// Send an ephemeral message that auto-deletes after expirySeconds
export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), expirySeconds: t.u32() },
  (ctx, { roomId, text, expirySeconds }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    if (expirySeconds < 1 || expirySeconds > 3600) throw new SenderError('Invalid expiry duration');

    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + BigInt(expirySeconds) * 1_000_000n;

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: expiresAtMicros,
      editedAt: undefined,
    });

    ctx.db.messageExpiry.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(expiresAtMicros),
      messageId: msg.id,
    });

    // Clear typing indicator
    for (const indicator of [...ctx.db.typingIndicator.by_room.filter(roomId)]) {
      if (indicator.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);
