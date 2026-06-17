import { t, SenderError } from 'spacetimedb/server';
import spacetimedb from './schema';

export { default } from './schema';

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
    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp });
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
