import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';

// Helper: returns true if ctx.sender is a member of the given room
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function isMember(ctx: any, roomId: bigint): boolean {
  for (const m of ctx.db.roomMember.room_member_user_id.filter(ctx.sender)) {
    if (m.roomId === roomId) return true;
  }
  return false;
}

// Set or update the calling user's display name
export const set_username = spacetimedb.reducer(
  'set_username', { username: t.string() },
  (ctx, { username }) => {
    const trimmed = username.trim();
    if (!trimmed) throw new SenderError('Username cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Username too long (max 32)');

    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, username: trimmed });
    } else {
      ctx.db.user.insert({ identity: ctx.sender, username: trimmed, isOnline: true });
    }
  }
);

// Create a new room and automatically join it
export const create_room = spacetimedb.reducer(
  'create_room', { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (!trimmed) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64)');

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.username) throw new SenderError('Set a username before creating rooms');

    const room = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });

    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userId: ctx.sender });
    ctx.db.userRoomRead.insert({ id: 0n, userId: ctx.sender, roomId: room.id, lastMessageId: 0n });
  }
);

// Join an existing room
export const join_room = spacetimedb.reducer(
  'join_room', { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.username) throw new SenderError('Set a username before joining rooms');

    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');

    if (isMember(ctx, roomId)) throw new SenderError('Already a member of this room');

    ctx.db.roomMember.insert({ id: 0n, roomId, userId: ctx.sender });
    ctx.db.userRoomRead.insert({ id: 0n, userId: ctx.sender, roomId, lastMessageId: 0n });
  }
);

// Leave a room
export const leave_room = spacetimedb.reducer(
  'leave_room', { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!isMember(ctx, roomId)) throw new SenderError('Not a member of this room');

    // Remove membership
    for (const m of ctx.db.roomMember.room_member_user_id.filter(ctx.sender)) {
      if (m.roomId === roomId) {
        ctx.db.roomMember.id.delete(m.id);
        break;
      }
    }

    // Clear typing indicator if in this room
    const typing = ctx.db.typingIndicator.userId.find(ctx.sender);
    if (typing && typing.roomId === roomId) {
      ctx.db.typingIndicator.userId.delete(ctx.sender);
    }

    // Remove read state
    for (const r of ctx.db.userRoomRead.user_room_read_user_id.filter(ctx.sender)) {
      if (r.roomId === roomId) {
        ctx.db.userRoomRead.id.delete(r.id);
        break;
      }
    }
  }
);

// Send a message to a room
export const send_message = spacetimedb.reducer(
  { roomId: t.u64(), content: t.string() },
  (ctx, { roomId, content }) => {
    if (!isMember(ctx, roomId)) throw new SenderError('Not a member of this room');

    const trimmed = content.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000)');

    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      content: trimmed,
      sentAt: ctx.timestamp,
    });

    // Clear typing indicator when message is sent
    ctx.db.typingIndicator.userId.delete(ctx.sender);
  }
);

// Signal that the user is typing in a room
export const set_typing = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!isMember(ctx, roomId)) return; // silent fail for typing

    const existing = ctx.db.typingIndicator.userId.find(ctx.sender);
    if (existing) {
      ctx.db.typingIndicator.userId.update({ ...existing, roomId });
    } else {
      ctx.db.typingIndicator.insert({ userId: ctx.sender, roomId });
    }
  }
);

// Clear typing indicator
export const stop_typing = spacetimedb.reducer((ctx) => {
  ctx.db.typingIndicator.userId.delete(ctx.sender);
});

// Update last-read message ID for a room (drives unread counts + read receipts)
export const mark_room_read = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!isMember(ctx, roomId)) return;

    let found = false;
    for (const r of ctx.db.userRoomRead.user_room_read_user_id.filter(ctx.sender)) {
      if (r.roomId === roomId) {
        if (messageId > r.lastMessageId) {
          ctx.db.userRoomRead.id.update({ ...r, lastMessageId: messageId });
        }
        found = true;
        break;
      }
    }

    if (!found) {
      ctx.db.userRoomRead.insert({ id: 0n, userId: ctx.sender, roomId, lastMessageId: messageId });
    }
  }
);

// Lifecycle: user connects — create or mark online
spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, isOnline: true });
  } else {
    ctx.db.user.insert({ identity: ctx.sender, username: '', isOnline: true });
  }
});

// Lifecycle: user disconnects — mark offline, clear typing
spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, isOnline: false });
  }
  ctx.db.typingIndicator.userId.delete(ctx.sender);
});
