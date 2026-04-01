import { SenderError, t } from 'spacetimedb/server';
import { spacetimedb, User, RoomMember, Message, TypingIndicator, ReadReceipt, UserRoomRead } from './schema';

// ── Lifecycle ──────────────────────────────────────────────────────────────

spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (!existing) {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: '',
      online: true,
      createdAt: ctx.timestamp,
    });
  } else {
    ctx.db.user.identity.update({ ...existing, online: true });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, online: false });
  }
  // Remove all typing indicators for this user
  for (const ti of ctx.db.typingIndicator.typing_user_identity.filter(ctx.sender)) {
    ctx.db.typingIndicator.id.delete(ti.id);
  }
});

// ── User reducers ─────────────────────────────────────────────────────────

export const set_name = spacetimedb.reducer(
  'set_name',
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, name: trimmed });
  }
);

// ── Room reducers ─────────────────────────────────────────────────────────

export const create_room = spacetimedb.reducer(
  'create_room',
  { name: t.string() },
  (ctx, { name }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long');

    const room = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });

    // Creator auto-joins
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: room.id,
      memberIdentity: ctx.sender,
      joinedAt: ctx.timestamp,
    });
  }
);

export const join_room = spacetimedb.reducer(
  'join_room',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');

    // Already a member?
    for (const m of ctx.db.roomMember.room_member_room_id.filter(roomId)) {
      if (m.memberIdentity.toHexString() === ctx.sender.toHexString()) {
        throw new SenderError('Already a member');
      }
    }

    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      memberIdentity: ctx.sender,
      joinedAt: ctx.timestamp,
    });
  }
);

export const leave_room = spacetimedb.reducer(
  'leave_room',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of ctx.db.roomMember.room_member_room_id.filter(roomId)) {
      if (m.memberIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
        // Clear typing indicator
        for (const ti of ctx.db.typingIndicator.typing_user_identity.filter(ctx.sender)) {
          if (ti.roomId === roomId) ctx.db.typingIndicator.id.delete(ti.id);
        }
        return;
      }
    }
    throw new SenderError('Not a member');
  }
);

// ── Message reducers ──────────────────────────────────────────────────────

export const send_message = spacetimedb.reducer(
  'send_message',
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    // Verify membership
    let isMember = false;
    for (const m of ctx.db.roomMember.room_member_room_id.filter(roomId)) {
      if (m.memberIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderId: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
    });

    // Auto mark sender as having read this message
    _markRead(ctx, roomId, msg.id);

    // Clear sender's typing indicator
    for (const ti of ctx.db.typingIndicator.typing_user_identity.filter(ctx.sender)) {
      if (ti.roomId === roomId) ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
);

// ── Typing indicators ─────────────────────────────────────────────────────

export const set_typing = spacetimedb.reducer(
  'set_typing',
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    // 5-second TTL
    const expiresAt = {
      microsSinceUnixEpoch: ctx.timestamp.microsSinceUnixEpoch + 5_000_000n,
    };

    let existingId: bigint | undefined;
    for (const ti of ctx.db.typingIndicator.typing_user_identity.filter(ctx.sender)) {
      if (ti.roomId === roomId) {
        existingId = ti.id;
        break;
      }
    }

    if (isTyping) {
      if (existingId !== undefined) {
        const existing = ctx.db.typingIndicator.id.find(existingId)!;
        ctx.db.typingIndicator.id.update({ ...existing, expiresAt });
      } else {
        ctx.db.typingIndicator.insert({
          id: 0n,
          roomId,
          userIdentity: ctx.sender,
          expiresAt,
        });
      }
    } else {
      if (existingId !== undefined) {
        ctx.db.typingIndicator.id.delete(existingId);
      }
    }
  }
);

// ── Read receipts ─────────────────────────────────────────────────────────

function _markRead(ctx: any, roomId: bigint, messageId: bigint) {
  const senderHex = ctx.sender.toHexString();

  // Record read receipt for this message if not already there
  let alreadySeen = false;
  for (const rr of ctx.db.readReceipt.read_receipt_message_id.filter(messageId)) {
    if (rr.userIdentity.toHexString() === senderHex) {
      alreadySeen = true;
      break;
    }
  }
  if (!alreadySeen) {
    ctx.db.readReceipt.insert({
      id: 0n,
      messageId,
      userIdentity: ctx.sender,
      seenAt: ctx.timestamp,
    });
  }

  // Update last-read position for unread counts
  let existingRead: any;
  for (const r of ctx.db.userRoomRead.user_room_read_identity.filter(ctx.sender)) {
    if (r.roomId === roomId) {
      existingRead = r;
      break;
    }
  }

  if (existingRead) {
    if (messageId > existingRead.lastReadMessageId) {
      ctx.db.userRoomRead.id.update({
        ...existingRead,
        lastReadMessageId: messageId,
        lastReadAt: ctx.timestamp,
      });
    }
  } else {
    ctx.db.userRoomRead.insert({
      id: 0n,
      userIdentity: ctx.sender,
      roomId,
      lastReadMessageId: messageId,
      lastReadAt: ctx.timestamp,
    });
  }
}

export const mark_read = spacetimedb.reducer(
  'mark_read',
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    _markRead(ctx, roomId, messageId);
  }
);
