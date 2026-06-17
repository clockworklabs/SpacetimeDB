import { SenderError, t } from 'spacetimedb/server';
import spacetimedb from './schema.js';
export { default } from './schema.js';

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

    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp });

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
