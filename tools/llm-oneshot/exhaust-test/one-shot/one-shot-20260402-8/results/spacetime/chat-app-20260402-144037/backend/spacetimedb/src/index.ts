import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  user, room, roomMember, roomBan,
  message, messageHistory, messageReaction,
  typingStatus, readReceipt, roomLastRead,
} from './schema';

// ==================== SCHEDULED TABLES ====================
// Must be defined here alongside their reducers (forward reference pattern)

const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => deliverScheduledMessage,
    indexes: [{ accessor: 'bySender', algorithm: 'btree', columns: ['sender'] }],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
  }
);

const ephemeralTimer = table(
  {
    name: 'ephemeral_timer',
    scheduled: (): any => expireEphemeralMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ==================== FULL SCHEMA ====================

const spacetimedb = schema({
  user,
  room,
  roomMember,
  roomBan,
  message,
  messageHistory,
  messageReaction,
  typingStatus,
  readReceipt,
  roomLastRead,
  scheduledMessage,
  ephemeralTimer,
});

export default spacetimedb;

// ==================== LIFECYCLE HOOKS ====================

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    const newStatus = existing.status === 'invisible' ? 'invisible' : 'online';
    ctx.db.user.identity.update({ ...existing, online: true, status: newStatus });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActive: ctx.timestamp });
  }
  // Clear typing statuses for disconnected user
  for (const ts of [...ctx.db.typingStatus.userIdentity.filter(ctx.sender)]) {
    ctx.db.typingStatus.id.delete(ts.id);
  }
});

// ==================== USER REDUCERS ====================

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (!trimmed) throw new SenderError('Name cannot be empty');
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

export const updateStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, status, lastActive: ctx.timestamp });
  }
);

// ==================== ROOM REDUCERS ====================

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (!trimmed) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');

    const newRoom = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: newRoom.id,
      userIdentity: ctx.sender,
      role: 'admin',
      joinedAt: ctx.timestamp,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    for (const ban of ctx.db.roomBan.roomId.filter(roomId)) {
      if (ban.userIdentity.equals(ctx.sender)) throw new SenderError('You are banned from this room');
    }
    for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
      if (member.userIdentity.equals(ctx.sender)) throw new SenderError('Already a member');
    }
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userIdentity: ctx.sender,
      role: 'member',
      joinedAt: ctx.timestamp,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
      if (member.userIdentity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(member.id);
        // Clear typing status
        clearTypingForUser(ctx, roomId, ctx.sender);
        return;
      }
    }
    throw new SenderError('Not a member of this room');
  }
);

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const callerMember = findMember(ctx, roomId, ctx.sender);
    if (!callerMember || callerMember.role !== 'admin') throw new SenderError('Not an admin');
    if (targetIdentity.equals(ctx.sender)) throw new SenderError('Cannot kick yourself');

    for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
      if (member.userIdentity.equals(targetIdentity)) {
        ctx.db.roomMember.id.delete(member.id);
        ctx.db.roomBan.insert({
          id: 0n,
          roomId,
          userIdentity: targetIdentity,
          bannedAt: ctx.timestamp,
        });
        clearTypingForUser(ctx, roomId, targetIdentity);
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const callerMember = findMember(ctx, roomId, ctx.sender);
    if (!callerMember || callerMember.role !== 'admin') throw new SenderError('Not an admin');

    for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
      if (member.userIdentity.equals(targetIdentity)) {
        ctx.db.roomMember.id.update({ ...member, role: 'admin' });
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

// ==================== MESSAGE REDUCERS ====================

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    if (!isMember(ctx, roomId, ctx.sender)) throw new SenderError('Not a member of this room');

    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      isEdited: false,
      isEphemeral: false,
      expiresAt: undefined,
    });
    clearTypingForUser(ctx, roomId, ctx.sender);
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSecs: t.u32() },
  (ctx, { roomId, text, durationSecs }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (!isMember(ctx, roomId, ctx.sender)) throw new SenderError('Not a member of this room');
    if (durationSecs < 1 || durationSecs > 3600) throw new SenderError('Duration must be 1-3600 seconds');

    const durationMicros = BigInt(durationSecs) * 1_000_000n;
    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + durationMicros;
    const expiresAt = new Timestamp(expiresAtMicros);

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      isEdited: false,
      isEphemeral: true,
      expiresAt,
    });

    ctx.db.ephemeralTimer.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAtMicros),
      messageId: msg.id,
    });
    clearTypingForUser(ctx, roomId, ctx.sender);
  }
);

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const trimmed = text.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');
    if (!isMember(ctx, roomId, ctx.sender)) throw new SenderError('Not a member of this room');
    if (sendAtMicros <= ctx.timestamp.microsSinceUnixEpoch) throw new SenderError('Scheduled time must be in the future');

    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
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
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    const trimmed = newText.trim();
    if (!trimmed) throw new SenderError('Message cannot be empty');

    ctx.db.messageHistory.insert({
      id: 0n,
      messageId,
      oldText: msg.text,
      editedAt: ctx.timestamp,
    });
    ctx.db.message.id.update({ ...msg, text: trimmed, isEdited: true });
  }
);

export const reactToMessage = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    for (const reaction of ctx.db.messageReaction.messageId.filter(messageId)) {
      if (reaction.userIdentity.equals(ctx.sender) && reaction.emoji === emoji) {
        ctx.db.messageReaction.id.delete(reaction.id);
        return;
      }
    }
    ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
  }
);

export const markRoomRead = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');

    let maxId = 0n;
    for (const msg of ctx.db.message.roomId.filter(roomId)) {
      if (msg.id > maxId) maxId = msg.id;
    }
    if (maxId === 0n) return;

    // Upsert roomLastRead
    let found = false;
    for (const lr of ctx.db.roomLastRead.userIdentity.filter(ctx.sender)) {
      if (lr.roomId === roomId) {
        if (lr.lastReadMessageId < maxId) {
          ctx.db.roomLastRead.id.update({ ...lr, lastReadMessageId: maxId });
        }
        found = true;
        break;
      }
    }
    if (!found) {
      ctx.db.roomLastRead.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: maxId });
    }

    // Add read receipt for latest message if not already present
    let hasReceipt = false;
    for (const receipt of ctx.db.readReceipt.messageId.filter(maxId)) {
      if (receipt.userIdentity.equals(ctx.sender)) {
        hasReceipt = true;
        break;
      }
    }
    if (!hasReceipt) {
      ctx.db.readReceipt.insert({
        id: 0n,
        messageId: maxId,
        userIdentity: ctx.sender,
        readAt: ctx.timestamp,
      });
    }
  }
);

// ==================== TYPING REDUCERS ====================

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    if (!isMember(ctx, roomId, ctx.sender)) return;

    for (const ts of ctx.db.typingStatus.roomId.filter(roomId)) {
      if (ts.userIdentity.equals(ctx.sender)) {
        ctx.db.typingStatus.id.update({ ...ts, lastTypedAt: ctx.timestamp });
        return;
      }
    }
    ctx.db.typingStatus.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastTypedAt: ctx.timestamp });
  }
);

export const stopTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    clearTypingForUser(ctx, roomId, ctx.sender);
  }
);

// ==================== SCHEDULED REDUCERS ====================

export const deliverScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    if (!isMember(ctx, timer.roomId, timer.sender)) return;
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      sender: timer.sender,
      text: timer.text,
      sentAt: ctx.timestamp,
      isEdited: false,
      isEphemeral: false,
      expiresAt: undefined,
    });
  }
);

export const expireEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralTimer.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.id.find(timer.messageId);
    if (msg) {
      ctx.db.message.id.delete(timer.messageId);
      // Clean up read receipts
      for (const receipt of ctx.db.readReceipt.messageId.filter(timer.messageId)) {
        ctx.db.readReceipt.id.delete(receipt.id);
      }
    }
  }
);

// ==================== HELPERS ====================

function isMember(ctx: any, roomId: bigint, identity: any): boolean {
  for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
    if (member.userIdentity.equals(identity)) return true;
  }
  return false;
}

function findMember(ctx: any, roomId: bigint, identity: any): any {
  for (const member of ctx.db.roomMember.roomId.filter(roomId)) {
    if (member.userIdentity.equals(identity)) return member;
  }
  return null;
}

function clearTypingForUser(ctx: any, roomId: bigint, identity: any): void {
  for (const ts of ctx.db.typingStatus.roomId.filter(roomId)) {
    if (ts.userIdentity.equals(identity)) {
      ctx.db.typingStatus.id.delete(ts.id);
      return;
    }
  }
}
