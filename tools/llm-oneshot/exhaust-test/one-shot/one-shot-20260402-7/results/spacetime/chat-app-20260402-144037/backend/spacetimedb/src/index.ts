import { t, SenderError } from 'spacetimedb/server';
import spacetimedb, { ScheduleAt } from './schema';
export { default } from './schema';
export { sendScheduledMessage, deleteEphemeralMessage } from './schema';

// ─── Lifecycle Hooks ──────────────────────────────────────────────────────────

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true, lastActive: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActive: ctx.timestamp });
  }
  // Remove typing indicators for disconnected user
  const allIndicators = [...ctx.db.typingIndicator.iter()];
  for (const ind of allIndicators) {
    if (ind.userId.equals(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(ind.id);
    }
  }
});

// ─── User Reducers ────────────────────────────────────────────────────────────

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Already registered');
    ctx.db.user.insert({
      identity: ctx.sender,
      name: trimmed,
      online: true,
      status: 'online',
      lastActive: ctx.timestamp,
    });
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...user, status, lastActive: ctx.timestamp });
  }
);

export const heartbeat = spacetimedb.reducer((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) return;
  ctx.db.user.identity.update({ ...user, lastActive: ctx.timestamp });
});

// ─── Room Reducers ────────────────────────────────────────────────────────────

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 chars)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const newRoom = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender });
    ctx.db.roomMember.insert({ id: 0n, roomId: newRoom.id, userId: ctx.sender, isAdmin: true });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const bans = [...ctx.db.roomBan.by_room.filter(roomId)];
    if (bans.some((b) => b.userId.equals(ctx.sender))) throw new SenderError('You are banned from this room');
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    if (members.some((m) => m.userId.equals(ctx.sender))) return;
    ctx.db.roomMember.insert({ id: 0n, roomId, userId: ctx.sender, isAdmin: false });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    const membership = members.find((m) => m.userId.equals(ctx.sender));
    if (!membership) return;
    ctx.db.roomMember.id.delete(membership.id);
    // Remove typing indicator
    const indicators = [...ctx.db.typingIndicator.by_room.filter(roomId)];
    const myIndicator = indicators.find((i) => i.userId.equals(ctx.sender));
    if (myIndicator) ctx.db.typingIndicator.id.delete(myIndicator.id);
  }
);

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    const callerMembership = members.find((m) => m.userId.equals(ctx.sender));
    if (!callerMembership?.isAdmin) throw new SenderError('Not an admin');
    const targetMembership = members.find((m) => m.userId.equals(userId));
    if (!targetMembership) throw new SenderError('User not in room');
    if (targetMembership.isAdmin) throw new SenderError('Cannot kick an admin');
    ctx.db.roomMember.id.delete(targetMembership.id);
    // Add to ban list
    const bans = [...ctx.db.roomBan.by_room.filter(roomId)];
    if (!bans.some((b) => b.userId.equals(userId))) {
      ctx.db.roomBan.insert({ id: 0n, roomId, userId });
    }
    // Remove typing indicator
    const indicators = [...ctx.db.typingIndicator.by_room.filter(roomId)];
    const theirIndicator = indicators.find((i) => i.userId.equals(userId));
    if (theirIndicator) ctx.db.typingIndicator.id.delete(theirIndicator.id);
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    const callerMembership = members.find((m) => m.userId.equals(ctx.sender));
    if (!callerMembership?.isAdmin) throw new SenderError('Not an admin');
    const targetMembership = members.find((m) => m.userId.equals(userId));
    if (!targetMembership) throw new SenderError('User not in room');
    ctx.db.roomMember.id.update({ ...targetMembership, isAdmin: true });
  }
);

// ─── Message Reducers ─────────────────────────────────────────────────────────

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), isEphemeral: t.bool(), durationSecs: t.u32() },
  (ctx, { roomId, text, isEphemeral, durationSecs }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    if (!members.some((m) => m.userId.equals(ctx.sender))) throw new SenderError('Not a member of this room');
    const expiresAt =
      isEphemeral && durationSecs > 0
        ? { microsSinceUnixEpoch: ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSecs) * 1_000_000n }
        : undefined;
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral,
      expiresAt,
    });
    if (isEphemeral && durationSecs > 0) {
      const delayMicros = BigInt(durationSecs) * 1_000_000n;
      ctx.db.ephemeralExpiry.insert({
        scheduledId: 0n,
        scheduledAt: ScheduleAt.time({ microsSinceUnixEpoch: ctx.timestamp.microsSinceUnixEpoch + delayMicros }),
        messageId: msg.id,
      });
    }
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    // Save current text to history
    ctx.db.messageEditHistory.insert({ id: 0n, messageId, text: msg.text, editedAt: ctx.timestamp });
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.i64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    if (!members.some((m) => m.userId.equals(ctx.sender))) throw new SenderError('Not a member of this room');
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time({ microsSinceUnixEpoch: BigInt(sendAtMicros) }),
      roomId,
      sender: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const msg = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!msg) throw new SenderError('Scheduled message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

export const reactToMessage = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');
    const reactions = [...ctx.db.messageReaction.by_message.filter(messageId)];
    const existing = reactions.find((r) => r.userId.equals(ctx.sender) && r.emoji === emoji);
    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userId: ctx.sender, emoji });
    }
  }
);

export const markRoomRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    const receipts = [...ctx.db.readReceipt.by_room.filter(roomId)];
    const existing = receipts.find((r) => r.userId.equals(ctx.sender));
    if (existing) {
      if (messageId > existing.messageId) {
        ctx.db.readReceipt.id.update({ ...existing, messageId });
      }
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userId: ctx.sender, messageId });
    }
  }
);

// ─── Typing Reducers ──────────────────────────────────────────────────────────

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const members = [...ctx.db.roomMember.by_room.filter(roomId)];
    if (!members.some((m) => m.userId.equals(ctx.sender))) return;
    const indicators = [...ctx.db.typingIndicator.by_room.filter(roomId)];
    const existing = indicators.find((i) => i.userId.equals(ctx.sender));
    if (existing) {
      ctx.db.typingIndicator.id.update({ ...existing, lastTypingAt: ctx.timestamp });
    } else {
      ctx.db.typingIndicator.insert({ id: 0n, roomId, userId: ctx.sender, lastTypingAt: ctx.timestamp });
    }
  }
);

export const stopTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const indicators = [...ctx.db.typingIndicator.by_room.filter(roomId)];
    const existing = indicators.find((i) => i.userId.equals(ctx.sender));
    if (existing) ctx.db.typingIndicator.id.delete(existing.id);
  }
);
