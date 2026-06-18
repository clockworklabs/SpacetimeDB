import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import spacetimedb from './schema';
export { default } from './schema';
export { processScheduledMessages, deleteExpiredMessages } from './schema';

export const init = spacetimedb.init((ctx) => {
  if ([...ctx.db.scheduledMessageTimer.iter()].length === 0) {
    ctx.db.scheduledMessageTimer.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.interval(10_000_000n),
    });
  }
  if ([...ctx.db.ephemeralMessageTimer.iter()].length === 0) {
    ctx.db.ephemeralMessageTimer.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.interval(10_000_000n),
    });
  }
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, status: 'online', lastActiveAt: ctx.timestamp });
  } else {
    ctx.db.user.insert({ identity: ctx.sender, name: '', status: 'online', lastActiveAt: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, status: 'offline', lastActiveAt: ctx.timestamp });
  }
  for (const indicator of [...ctx.db.typingIndicator.userIdentity.filter(ctx.sender)]) {
    ctx.db.typingIndicator.id.delete(indicator.id);
  }
});

export const setName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 32) {
      throw new SenderError('Name must be 1-32 characters');
    }
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, name: trimmed });
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const allowed = ['online', 'away', 'dnd', 'invisible'];
    if (!allowed.includes(status)) throw new SenderError('Invalid status');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, status, lastActiveAt: ctx.timestamp });
  }
);

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 64) {
      throw new SenderError('Room name must be 1-64 characters');
    }
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    const room = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });

    ctx.db.roomMember.insert({
      id: 0n,
      roomId: room.id,
      userIdentity: ctx.sender,
      isAdmin: true,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    const banned = [...ctx.db.roomBan.by_room_user.filter([roomId, ctx.sender])];
    if (banned.length > 0) throw new SenderError('You are banned from this room');

    const existing = [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])];
    if (existing.length > 0) return;

    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userIdentity: ctx.sender,
      isAdmin: false,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const member of [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])]) {
      ctx.db.roomMember.id.delete(member.id);
    }
    for (const indicator of [...ctx.db.typingIndicator.by_room_user.filter([roomId, ctx.sender])]) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }
);

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), content: t.string(), ttlSeconds: t.option(t.u32()), parentMessageId: t.option(t.u64()) },
  (ctx, { roomId, content, ttlSeconds, parentMessageId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');

    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    const membership = [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])];
    if (membership.length === 0) throw new SenderError('Not a member of this room');

    const trimmed = content.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    if (parentMessageId !== undefined) {
      const parent = ctx.db.message.id.find(parentMessageId);
      if (!parent) throw new SenderError('Parent message not found');
      if (parent.roomId !== roomId) throw new SenderError('Parent message is in a different room');
    }

    const expiresAt = ttlSeconds !== undefined
      ? ctx.timestamp.microsSinceUnixEpoch + BigInt(ttlSeconds) * 1_000_000n
      : undefined;

    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      content: trimmed,
      sentAt: ctx.timestamp,
      expiresAt,
      parentMessageId,
    });
  }
);

export const updateTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    const membership = [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])];
    if (membership.length === 0) return;

    const existing = [...ctx.db.typingIndicator.by_room_user.filter([roomId, ctx.sender])];

    if (isTyping) {
      if (existing.length > 0) {
        ctx.db.typingIndicator.id.update({ ...existing[0], updatedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.insert({
          id: 0n,
          roomId,
          userIdentity: ctx.sender,
          updatedAt: ctx.timestamp,
        });
      }
    } else {
      for (const indicator of existing) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    const membership = [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])];
    if (membership.length === 0) return;

    const existing = [...ctx.db.userRoomRead.by_room_user.filter([roomId, ctx.sender])];

    if (existing.length > 0) {
      if (messageId > existing[0].lastReadMessageId) {
        ctx.db.userRoomRead.id.update({ ...existing[0], lastReadMessageId: messageId });
      }
    } else {
      ctx.db.userRoomRead.insert({
        id: 0n,
        roomId,
        userIdentity: ctx.sender,
        lastReadMessageId: messageId,
      });
    }
  }
);

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), content: t.string(), sendAt: t.u64() },
  (ctx, { roomId, content, sendAt }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || user.name === '') throw new SenderError('Set a display name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const membership = [...ctx.db.roomMember.by_room_user.filter([roomId, ctx.sender])];
    if (membership.length === 0) throw new SenderError('Not a member of this room');
    const trimmed = content.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 characters');
    if (sendAt <= ctx.timestamp.microsSinceUnixEpoch) throw new SenderError('Scheduled time must be in the future');

    ctx.db.scheduledMessage.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      content: trimmed,
      sendAt,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const pending = ctx.db.scheduledMessage.id.find(id);
    if (!pending) return;
    if (!pending.sender.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.id.delete(id);
  }
);

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');
    const existing = [...ctx.db.messageReaction.by_message.filter(messageId)]
      .find(r => r.userIdentity.equals(ctx.sender) && r.emoji === emoji);
    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), content: t.string() },
  (ctx, { messageId, content }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');

    const trimmed = content.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    ctx.db.messageEditHistory.insert({
      id: 0n,
      messageId: msg.id,
      previousContent: msg.content,
      editedAt: ctx.timestamp,
    });

    ctx.db.message.id.update({ ...msg, content: trimmed });
  }
);

export const kickUser = spacetimedb.reducer(
  { memberId: t.u64() },
  (ctx, { memberId }) => {
    const member = ctx.db.roomMember.id.find(memberId);
    if (!member) throw new SenderError('Member not found');

    const callerMemberships = [...ctx.db.roomMember.by_room_user.filter([member.roomId, ctx.sender])];
    if (callerMemberships.length === 0 || !callerMemberships[0].isAdmin) {
      throw new SenderError('Not an admin of this room');
    }
    if (member.isAdmin) throw new SenderError('Cannot kick an admin');

    ctx.db.roomMember.id.delete(memberId);

    const alreadyBanned = [...ctx.db.roomBan.by_room_user.filter([member.roomId, member.userIdentity])];
    if (alreadyBanned.length === 0) {
      ctx.db.roomBan.insert({ id: 0n, roomId: member.roomId, userIdentity: member.userIdentity });
    }

    for (const ti of [...ctx.db.typingIndicator.by_room_user.filter([member.roomId, member.userIdentity])]) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
);

export const promoteToAdmin = spacetimedb.reducer(
  { memberId: t.u64() },
  (ctx, { memberId }) => {
    const member = ctx.db.roomMember.id.find(memberId);
    if (!member) throw new SenderError('Member not found');

    const callerMemberships = [...ctx.db.roomMember.by_room_user.filter([member.roomId, ctx.sender])];
    if (callerMemberships.length === 0 || !callerMemberships[0].isAdmin) {
      throw new SenderError('Not an admin of this room');
    }

    ctx.db.roomMember.id.update({ ...member, isAdmin: true });
  }
);
