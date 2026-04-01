import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
export { default } from './schema';

// --- Lifecycle Hooks ---

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
  // Clean up typing indicators for disconnected user
  for (const ti of [...ctx.db.typingIndicator.iter()]) {
    if (ti.userIdentity.equals(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
});

// --- User ---

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 50) throw new SenderError('Name too long');
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Already registered');
    // Check for duplicate name
    for (const u of ctx.db.user.iter()) {
      if (u.name === trimmed) throw new SenderError('Name already taken');
    }
    ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true });
  }
);

// --- Rooms ---

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 100) throw new SenderError('Room name too long');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    // Check duplicate room name
    for (const r of ctx.db.room.iter()) {
      if (r.name === trimmed) throw new SenderError('Room already exists');
    }
    const room = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender });
    // Auto-join creator
    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    // Check already joined
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.userIdentity.equals(ctx.sender)) throw new SenderError('Already in room');
    }
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(m.id);
        // Clean up typing indicators
        for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
          if (ti.userIdentity.equals(ctx.sender)) {
            ctx.db.typingIndicator.id.delete(ti.id);
          }
        }
        // Clean up read receipts
        for (const rr of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
          if (rr.userIdentity.equals(ctx.sender)) {
            ctx.db.readReceipt.id.delete(rr.id);
          }
        }
        return;
      }
    }
    throw new SenderError('Not in room');
  }
);

// --- Messages ---

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    // Check membership
    let isMember = false;
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.userIdentity.equals(ctx.sender)) { isMember = true; break; }
    }
    if (!isMember) throw new SenderError('Not a member of this room');
    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      createdAt: BigInt(ctx.timestamp.microsSinceUnixEpoch),
    });
    // Clear typing indicator on send
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

// --- Typing Indicators ---

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    // Check membership
    let isMember = false;
    for (const m of ctx.db.roomMember.roomId.filter(roomId)) {
      if (m.userIdentity.equals(ctx.sender)) { isMember = true; break; }
    }
    if (!isMember) throw new SenderError('Not a member of this room');
    // Update existing or insert new
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.update({
          ...ti,
          startedAt: BigInt(ctx.timestamp.microsSinceUnixEpoch),
        });
        return;
      }
    }
    ctx.db.typingIndicator.insert({
      id: 0n,
      roomId,
      userIdentity: ctx.sender,
      startedAt: BigInt(ctx.timestamp.microsSinceUnixEpoch),
    });
  }
);

export const clearTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

// --- Read Receipts ---

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    // Update existing or insert new
    for (const rr of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (rr.userIdentity.equals(ctx.sender)) {
        if (messageId > rr.lastReadMessageId) {
          ctx.db.readReceipt.id.update({ ...rr, lastReadMessageId: messageId });
        }
        return;
      }
    }
    ctx.db.readReceipt.insert({
      id: 0n,
      roomId,
      userIdentity: ctx.sender,
      lastReadMessageId: messageId,
    });
  }
);
