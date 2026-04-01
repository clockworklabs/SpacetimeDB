import spacetimedb, { TypingIndicator, ReadReceipt } from './schema';
import { t, SenderError } from 'spacetimedb/server';

export { default } from './schema';

// Lifecycle: client connected
spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true, lastSeen: ctx.timestamp });
  } else {
    ctx.db.user.insert({ identity: ctx.sender, name: '', online: true, lastSeen: ctx.timestamp });
  }
});

// Lifecycle: client disconnected
spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, online: false, lastSeen: ctx.timestamp });
  }
  // Remove typing indicators for disconnected user
  for (const ti of ctx.db.typing_indicator.iter()) {
    if (ti.identity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typing_indicator.id.delete(ti.id);
    }
  }
});

// Set display name — reducer name comes from export
export const set_name = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
  const trimmed = name.trim();
  if (!trimmed) throw new SenderError('Name cannot be empty');
  if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');
  ctx.db.user.identity.update({ ...user, name: trimmed });
});

// Create a room
export const create_room = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
  const trimmed = name.trim();
  if (!trimmed) throw new SenderError('Room name cannot be empty');
  if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 chars)');

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) throw new SenderError('Set a display name first');

  const room = ctx.db.room.insert({
    id: 0n,
    name: trimmed,
    createdBy: ctx.sender,
    createdAt: ctx.timestamp,
  });

  // Auto-join the creator
  ctx.db.room_member.insert({
    id: 0n,
    roomId: room.id,
    identity: ctx.sender,
    joinedAt: ctx.timestamp,
  });
});

// Join a room
export const join_room = spacetimedb.reducer({ roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');

  // Check if already a member
  for (const m of ctx.db.room_member.room_member_room_id.filter(roomId)) {
    if (m.identity.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Already in room');
    }
  }

  ctx.db.room_member.insert({
    id: 0n,
    roomId,
    identity: ctx.sender,
    joinedAt: ctx.timestamp,
  });
});

// Leave a room
export const leave_room = spacetimedb.reducer({ roomId: t.u64() }, (ctx, { roomId }) => {
  for (const m of ctx.db.room_member.room_member_room_id.filter(roomId)) {
    if (m.identity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.room_member.id.delete(m.id);
      return;
    }
  }
  throw new SenderError('Not a member of this room');
});

// Send a message
export const send_message = spacetimedb.reducer({ roomId: t.u64(), text: t.string() }, (ctx, { roomId, text }) => {
  const trimmed = text.trim();
  if (!trimmed) throw new SenderError('Message cannot be empty');
  if (trimmed.length > 1000) throw new SenderError('Message too long (max 1000 chars)');

  // Must be a member
  let isMember = false;
  for (const m of ctx.db.room_member.room_member_room_id.filter(roomId)) {
    if (m.identity.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Must join room first');

  ctx.db.message.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    text: trimmed,
    sentAt: ctx.timestamp,
  });

  // Clear typing indicator on send
  for (const ti of ctx.db.typing_indicator.typing_room_id.filter(roomId)) {
    if (ti.identity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typing_indicator.id.delete(ti.id);
    }
  }
});

// Set typing indicator
export const set_typing = spacetimedb.reducer({ roomId: t.u64(), isTyping: t.bool() }, (ctx, { roomId, isTyping }) => {
  if (isTyping) {
    let existing: typeof TypingIndicator.rowType | undefined;
    for (const ti of ctx.db.typing_indicator.typing_room_id.filter(roomId)) {
      if (ti.identity.toHexString() === ctx.sender.toHexString()) {
        existing = ti;
        break;
      }
    }

    if (existing) {
      ctx.db.typing_indicator.id.update({ ...existing, updatedAt: ctx.timestamp });
    } else {
      ctx.db.typing_indicator.insert({
        id: 0n,
        roomId,
        identity: ctx.sender,
        updatedAt: ctx.timestamp,
      });
    }
  } else {
    for (const ti of ctx.db.typing_indicator.typing_room_id.filter(roomId)) {
      if (ti.identity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typing_indicator.id.delete(ti.id);
      }
    }
  }
});

// Mark messages as read up to a given message ID
export const mark_read = spacetimedb.reducer({ roomId: t.u64(), messageId: t.u64() }, (ctx, { roomId, messageId }) => {
  let existing: typeof ReadReceipt.rowType | undefined;
  for (const rr of ctx.db.read_receipt.read_receipt_room_id.filter(roomId)) {
    if (rr.identity.toHexString() === ctx.sender.toHexString()) {
      existing = rr;
      break;
    }
  }

  if (existing) {
    if (messageId > existing.lastReadMessageId) {
      ctx.db.read_receipt.id.update({ ...existing, lastReadMessageId: messageId, readAt: ctx.timestamp });
    }
  } else {
    ctx.db.read_receipt.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      lastReadMessageId: messageId,
      readAt: ctx.timestamp,
    });
  }
});
