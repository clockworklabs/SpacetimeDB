import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
export { default, sendScheduledMessage, deleteExpiredMessage } from './schema';

// Lifecycle hooks
export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    // Restore online=true, but only if not invisible
    const online = existing.status !== 'invisible';
    ctx.db.user.identity.update({ ...existing, online });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActiveAt: ctx.timestamp });
  }
  // Clear typing indicators for this user
  const indicators = [...ctx.db.typingIndicator.userIdentity.filter(ctx.sender)];
  for (const indicator of indicators) {
    ctx.db.typingIndicator.id.delete(indicator.id);
  }
});

// Register / set name
export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 characters)');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, name: trimmed, online: true, status: existing.status || 'online' });
    } else {
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true, status: 'online', lastActiveAt: ctx.timestamp });
    }
  }
);

// Create a room
export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long');
    const newRoom = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp });
    // Creator automatically joins as admin
    ctx.db.roomMember.insert({ id: 0n, roomId: newRoom.id, userIdentity: ctx.sender, isAdmin: true });
  }
);

// Join a room
export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    // Check if banned
    const banned = [...ctx.db.roomBan.roomId.filter(roomId)].find(
      b => b.userIdentity.toHexString() === ctx.sender.toHexString()
    );
    if (banned) throw new SenderError('You have been banned from this room');
    // Check if already a member
    const existing = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === ctx.sender.toHexString()
    );
    if (!existing) {
      ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, isAdmin: false });
    }
  }
);

// Leave a room
export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const members = [...ctx.db.roomMember.roomId.filter(roomId)];
    const member = members.find(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (member) {
      ctx.db.roomMember.id.delete(member.id);
    }
    // Clear typing indicator
    const indicators = [...ctx.db.typingIndicator.roomId.filter(roomId)];
    const myIndicator = indicators.find(i => i.userIdentity.toHexString() === ctx.sender.toHexString());
    if (myIndicator) {
      ctx.db.typingIndicator.id.delete(myIndicator.id);
    }
  }
);

// Send a message
export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    // Check membership
    const members = [...ctx.db.roomMember.roomId.filter(roomId)];
    const isMember = members.some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAtMicros: 0n });
    // Clear typing indicator
    const indicators = [...ctx.db.typingIndicator.roomId.filter(roomId)];
    const myIndicator = indicators.find(i => i.userIdentity.toHexString() === ctx.sender.toHexString());
    if (myIndicator) {
      ctx.db.typingIndicator.id.delete(myIndicator.id);
    }
  }
);

// Update typing indicator
export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const indicators = [...ctx.db.typingIndicator.roomId.filter(roomId)];
    const existing = indicators.find(i => i.userIdentity.toHexString() === ctx.sender.toHexString());
    if (isTyping) {
      if (existing) {
        ctx.db.typingIndicator.id.update({ ...existing, updatedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.insert({ id: 0n, roomId, userIdentity: ctx.sender, updatedAt: ctx.timestamp });
      }
    } else {
      if (existing) {
        ctx.db.typingIndicator.id.delete(existing.id);
      }
    }
  }
);

// Schedule a message to be sent at a future time
export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    const members = [...ctx.db.roomMember.roomId.filter(roomId)];
    const isMember = members.some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
      roomId,
      sender: ctx.sender,
      text: trimmed,
    });
  }
);

// Cancel a scheduled message
export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const scheduled = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!scheduled) throw new SenderError('Scheduled message not found');
    if (scheduled.sender.toHexString() !== ctx.sender.toHexString()) throw new SenderError('Not authorized');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// Send an ephemeral message that auto-deletes after a set duration
export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSeconds: t.u32() },
  (ctx, { roomId, text, durationSeconds }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    const members = [...ctx.db.roomMember.roomId.filter(roomId)];
    const isMember = members.some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    if (durationSeconds < 10 || durationSeconds > 3600) throw new SenderError('Duration must be 10s–3600s');

    const durationMicros = BigInt(durationSeconds) * 1_000_000n;
    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + durationMicros;

    const msg = ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAtMicros });
    ctx.db.messageExpiryTimer.insert({ scheduledId: 0n, scheduledAt: ScheduleAt.time(expiresAtMicros), messageId: msg.id });

    // Clear typing indicator
    const indicators = [...ctx.db.typingIndicator.roomId.filter(roomId)];
    const myIndicator = indicators.find(i => i.userIdentity.toHexString() === ctx.sender.toHexString());
    if (myIndicator) {
      ctx.db.typingIndicator.id.delete(myIndicator.id);
    }
  }
);

// Toggle a reaction on a message (add if not present, remove if already reacted with same emoji)
export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), roomId: t.u64(), emoji: t.string() },
  (ctx, { messageId, roomId, emoji }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    const members = [...ctx.db.roomMember.roomId.filter(roomId)];
    const isMember = members.some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    if (emoji.length === 0 || emoji.length > 8) throw new SenderError('Invalid emoji');
    // Check if already reacted
    const existing = [...ctx.db.messageReaction.messageId.filter(messageId)].find(
      r => r.userIdentity.toHexString() === ctx.sender.toHexString() && r.emoji === emoji
    );
    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, roomId, userIdentity: ctx.sender, emoji });
    }
  }
);

// Edit a message (owner only) and store history
export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (msg.sender.toHexString() !== ctx.sender.toHexString()) throw new SenderError('Can only edit own messages');
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    ctx.db.messageEdit.insert({ id: 0n, messageId, editedBy: ctx.sender, oldText: msg.text, newText: trimmed, editedAt: ctx.timestamp });
    ctx.db.message.id.update({ ...msg, text: trimmed });
  }
);

// Kick (ban) a user from a room — admin only
export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentityHex: t.string() },
  (ctx, { roomId, targetIdentityHex }) => {
    const adminMember = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === ctx.sender.toHexString()
    );
    if (!adminMember || !adminMember.isAdmin) throw new SenderError('Not an admin of this room');
    // Cannot kick self
    if (targetIdentityHex === ctx.sender.toHexString()) throw new SenderError('Cannot kick yourself');
    // Remove target from room members
    const targetMember = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === targetIdentityHex
    );
    if (!targetMember) throw new SenderError('User is not a member of this room');
    ctx.db.roomMember.id.delete(targetMember.id);
    // Add to ban list
    const alreadyBanned = [...ctx.db.roomBan.roomId.filter(roomId)].find(
      b => b.userIdentity.toHexString() === targetIdentityHex
    );
    if (!alreadyBanned) {
      ctx.db.roomBan.insert({ id: 0n, roomId, userIdentity: targetMember.userIdentity });
    }
    // Remove their typing indicator
    const indicators = [...ctx.db.typingIndicator.roomId.filter(roomId)];
    const targetIndicator = indicators.find(i => i.userIdentity.toHexString() === targetIdentityHex);
    if (targetIndicator) {
      ctx.db.typingIndicator.id.delete(targetIndicator.id);
    }
  }
);

// Promote a user to admin in a room — admin only
export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentityHex: t.string() },
  (ctx, { roomId, targetIdentityHex }) => {
    const adminMember = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === ctx.sender.toHexString()
    );
    if (!adminMember || !adminMember.isAdmin) throw new SenderError('Not an admin of this room');
    const targetMember = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === targetIdentityHex
    );
    if (!targetMember) throw new SenderError('User is not a member of this room');
    if (targetMember.isAdmin) throw new SenderError('User is already an admin');
    ctx.db.roomMember.id.update({ ...targetMember, isAdmin: true });
  }
);

// Set user status (online, away, dnd, invisible)
export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const allowed = ['online', 'away', 'dnd', 'invisible'];
    if (!allowed.includes(status)) throw new SenderError('Invalid status');
    // Invisible users appear offline (online=false)
    const online = status !== 'invisible';
    ctx.db.user.identity.update({ ...user, status, online, lastActiveAt: ctx.timestamp });
  }
);

// Mark messages as read up to a given message ID
export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), lastReadMessageId: t.u64() },
  (ctx, { roomId, lastReadMessageId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    const receipts = [...ctx.db.readReceipt.roomId.filter(roomId)];
    const existing = receipts.find(r => r.userIdentity.toHexString() === ctx.sender.toHexString());
    if (existing) {
      if (lastReadMessageId > existing.lastReadMessageId) {
        ctx.db.readReceipt.id.update({ ...existing, lastReadMessageId });
      }
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId });
    }
  }
);
