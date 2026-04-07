import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
export { default } from './schema';
export { sendScheduledMessage, deleteExpiredMessage } from './schema';

// Lifecycle hooks
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
    // Clear typing indicators for this user
    for (const ti of [...ctx.db.typingIndicator.userIdentity.filter(ctx.sender)]) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
});

// Set or update display name
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
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true, createdAt: ctx.timestamp });
    }
  }
);

// Create a room
export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 chars)');

    // Check for duplicate name
    const existing = ctx.db.room.name.find(trimmed);
    if (existing) throw new SenderError('Room already exists');

    const roomId = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp }).id;
    // Auto-join the creator
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
  }
);

// Join a room
export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Check if already a member
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        throw new SenderError('Already a member');
      }
    }
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
  }
);

// Leave a room
export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
        // Clear typing indicator if any
        for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
          if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
            ctx.db.typingIndicator.id.delete(ti.id);
          }
        }
        return;
      }
    }
    throw new SenderError('Not a member of this room');
  }
);

// Send a message
export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Verify membership
    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');

    const msg = ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAt: null, editedAt: null });

    // Update sender's read receipt to this message
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }
    if (found) {
      ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    }

    // Clear typing indicator for sender in this room
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

// Send an ephemeral message that auto-deletes after durationSecs seconds
export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSecs: t.u32() },
  (ctx, { roomId, text, durationSecs }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (durationSecs < 1 || durationSecs > 86400) throw new SenderError('Invalid duration');

    const expiryMicros = ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSecs) * 1_000_000n;

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: new Timestamp(expiryMicros),
      editedAt: null,
    });

    // Update sender's read receipt
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }
    if (found) {
      ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    }

    // Clear typing indicator
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }

    // Schedule deletion
    ctx.db.messageExpiry.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiryMicros),
      messageId: msg.id,
    });
  }
);

// Edit a message and save previous version to history
export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (msg.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Can only edit your own messages');
    }

    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (trimmed === msg.text) return; // No change

    // Save previous version to history
    ctx.db.messageEdit.insert({ id: 0n, messageId, previousText: msg.text, editedAt: ctx.timestamp });

    // Update the message
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// Set typing indicator
export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    if (!ctx.db.room.id.find(roomId)) return;

    // Find existing
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = ti;
        break;
      }
    }

    if (isTyping) {
      if (found) {
        ctx.db.typingIndicator.id.update({ ...found, updatedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.insert({ id: 0n, roomId, userIdentity: ctx.sender, updatedAt: ctx.timestamp });
      }
    } else {
      if (found) {
        ctx.db.typingIndicator.id.delete(found.id);
      }
    }
  }
);

// Mark messages as read up to a given message ID
export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;

    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }

    if (found) {
      if (messageId > found.lastReadMessageId) {
        ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: messageId, updatedAt: ctx.timestamp });
      }
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: messageId, updatedAt: ctx.timestamp });
    }
  }
);

// Schedule a message to be sent at a future time
export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtMicros: t.u64() },
  (ctx, { roomId, text, scheduledAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (scheduledAtMicros <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }

    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(scheduledAtMicros),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

// Cancel a pending scheduled message
export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const row = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!row) throw new SenderError('Scheduled message not found');
    if (row.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Not your scheduled message');
    }
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// Toggle a reaction on a message (add if not present, remove if already reacted with same emoji)
export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    const validEmojis = ['👍', '❤️', '😂', '😮', '😢'];
    if (!validEmojis.includes(emoji)) throw new SenderError('Invalid emoji');

    // Check if user already reacted with this emoji
    let found: { id: bigint; messageId: bigint; userIdentity: { toHexString(): string }; emoji: string } | undefined;
    for (const r of [...ctx.db.messageReaction.messageId.filter(messageId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString() && r.emoji === emoji) {
        found = r;
        break;
      }
    }

    if (found) {
      // Remove reaction
      ctx.db.messageReaction.id.delete(found.id);
    } else {
      // Add reaction
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);
