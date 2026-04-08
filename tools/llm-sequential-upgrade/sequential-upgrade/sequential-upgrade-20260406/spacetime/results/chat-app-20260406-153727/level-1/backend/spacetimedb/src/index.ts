import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
export { default } from './schema';

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

    const msg = ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp });

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
