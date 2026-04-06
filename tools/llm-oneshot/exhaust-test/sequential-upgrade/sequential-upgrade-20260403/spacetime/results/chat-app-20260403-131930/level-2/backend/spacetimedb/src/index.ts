import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
export { default, sendScheduledMessage } from './schema';

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
      ctx.db.user.identity.update({ ...existing, name: trimmed, online: true });
    } else {
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true });
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
    ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp });
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
    // Check if already a member
    const existing = [...ctx.db.roomMember.roomId.filter(roomId)].find(
      m => m.userIdentity.toHexString() === ctx.sender.toHexString()
    );
    if (!existing) {
      ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender });
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
    ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp });
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
