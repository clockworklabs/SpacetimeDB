import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  user, room, roomMember, message, messageEditHistory,
  typingIndicator, lastRead, reaction,
} from './schema';

// Scheduled tables (defined here alongside their reducers to avoid circular deps)
const scheduledMessageDelivery = table({
  name: 'scheduled_message_delivery',
  public: true,
  scheduled: (): any => deliverScheduledMessage,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  roomId: t.u64(),
  sender: t.identity(),
  text: t.string(),
});

const ephemeralDeleteJob = table({
  name: 'ephemeral_delete_job',
  scheduled: (): any => deleteEphemeralMessage,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  messageId: t.u64(),
});

const typingCleanupTimer = table({
  name: 'typing_cleanup_timer',
  scheduled: (): any => cleanupTypingIndicators,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const awayChecker = table({
  name: 'away_checker',
  scheduled: (): any => checkAway,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({
  user, room, roomMember, message, messageEditHistory,
  typingIndicator, lastRead, reaction,
  scheduledMessageDelivery, ephemeralDeleteJob, typingCleanupTimer, awayChecker,
});
export default spacetimedb;

// ─── Lifecycle hooks ──────────────────────────────────────────────────────────

export const init = spacetimedb.init((ctx) => {
  if ([...ctx.db.typingCleanupTimer.iter()].length === 0) {
    ctx.db.typingCleanupTimer.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(5_000_000n), // every 5 seconds
    });
  }
  if ([...ctx.db.awayChecker.iter()].length === 0) {
    ctx.db.awayChecker.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(60_000_000n), // every 60 seconds
    });
  }
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({
      ...existing,
      online: true,
      status: existing.status === 'invisible' ? 'invisible' : 'online',
      lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({
      ...existing,
      online: false,
      lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  // Clear typing indicators
  const typing = [...ctx.db.typingIndicator.iter()].find(ti => ti.userId.equals(ctx.sender));
  if (typing) ctx.db.typingIndicator.id.delete(typing.id);
});

// ─── User management ──────────────────────────────────────────────────────────

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed === '') throw new SenderError('Name cannot be empty');
    if (trimmed.length > 30) throw new SenderError('Name too long (max 30 chars)');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({
        ...existing,
        name: trimmed,
        online: true,
        lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    } else {
      ctx.db.user.insert({
        identity: ctx.sender,
        name: trimmed,
        online: true,
        status: 'online',
        lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({
      ...existing,
      status,
      lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
);

// ─── Room management ──────────────────────────────────────────────────────────

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed === '') throw new SenderError('Room name cannot be empty');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const newRoom = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender });
    ctx.db.roomMember.insert({
      id: 0n, roomId: newRoom.id, userId: ctx.sender, isAdmin: true, isBanned: false,
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const existing = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (existing) {
      if (existing.isBanned) throw new SenderError('You are banned from this room');
      return;
    }
    ctx.db.roomMember.insert({
      id: 0n, roomId, userId: ctx.sender, isAdmin: false, isBanned: false,
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const member = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!member) return;
    ctx.db.roomMember.id.delete(member.id);
    const typing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userId.equals(ctx.sender));
    if (typing) ctx.db.typingIndicator.id.delete(typing.id);
  }
);

// ─── Messaging ────────────────────────────────────────────────────────────────

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const trimmed = text.trim();
    if (trimmed === '') throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const member = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!member) throw new SenderError('Not a member of this room');
    if (member.isBanned) throw new SenderError('You are banned');
    ctx.db.message.insert({
      id: 0n, roomId, sender: ctx.sender, text: trimmed,
      sentAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      isEdited: false, isEphemeral: false, expiresAtMicros: 0n,
    });
    // Update activity and clear typing
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (userRow) {
      ctx.db.user.identity.update({
        ...userRow,
        lastActiveMicros: ctx.timestamp.microsSinceUnixEpoch,
        status: userRow.status === 'away' ? 'online' : userRow.status,
      });
    }
    const typing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userId.equals(ctx.sender));
    if (typing) ctx.db.typingIndicator.id.delete(typing.id);
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationMicros: t.u64() },
  (ctx, { roomId, text, durationMicros }) => {
    const trimmed = text.trim();
    if (trimmed === '') throw new SenderError('Message cannot be empty');
    const member = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!member) throw new SenderError('Not a member');
    if (member.isBanned) throw new SenderError('You are banned');
    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + durationMicros;
    const msg = ctx.db.message.insert({
      id: 0n, roomId, sender: ctx.sender, text: trimmed,
      sentAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      isEdited: false, isEphemeral: true, expiresAtMicros,
    });
    ctx.db.ephemeralDeleteJob.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAtMicros),
      messageId: msg.id,
    });
  }
);

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const trimmed = text.trim();
    if (trimmed === '') throw new SenderError('Message cannot be empty');
    const member = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!member) throw new SenderError('Not a member');
    if (member.isBanned) throw new SenderError('You are banned');
    if (sendAtMicros <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }
    ctx.db.scheduledMessageDelivery.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
      roomId, sender: ctx.sender, text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const scheduled = ctx.db.scheduledMessageDelivery.scheduledId.find(scheduledId);
    if (!scheduled) throw new SenderError('Scheduled message not found');
    if (!scheduled.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    ctx.db.scheduledMessageDelivery.scheduledId.delete(scheduledId);
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (trimmed === '') throw new SenderError('Message cannot be empty');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    ctx.db.messageEditHistory.insert({
      id: 0n, messageId, text: msg.text,
      editedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
    ctx.db.message.id.update({ ...msg, text: trimmed, isEdited: true });
  }
);

// ─── Typing indicators ────────────────────────────────────────────────────────

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const member = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!member || member.isBanned) return;
    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
    const existing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userId.equals(ctx.sender));
    if (existing) {
      ctx.db.typingIndicator.id.update({ ...existing, expiresAtMicros });
    } else {
      ctx.db.typingIndicator.insert({ id: 0n, roomId, userId: ctx.sender, expiresAtMicros });
    }
  }
);

export const clearTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const existing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userId.equals(ctx.sender));
    if (existing) ctx.db.typingIndicator.id.delete(existing.id);
  }
);

// ─── Read receipts ────────────────────────────────────────────────────────────

export const markRoomRead = spacetimedb.reducer(
  { roomId: t.u64(), lastMessageId: t.u64() },
  (ctx, { roomId, lastMessageId }) => {
    const existing = [...ctx.db.lastRead.roomId.filter(roomId)]
      .find(r => r.userId.equals(ctx.sender));
    if (existing) {
      if (lastMessageId > existing.lastMessageId) {
        ctx.db.lastRead.id.update({ ...existing, lastMessageId });
      }
    } else {
      ctx.db.lastRead.insert({ id: 0n, roomId, userId: ctx.sender, lastMessageId });
    }
  }
);

// ─── Reactions ────────────────────────────────────────────────────────────────

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const existing = [...ctx.db.reaction.messageId.filter(messageId)]
      .find(r => r.userId.equals(ctx.sender) && r.emoji === emoji);
    if (existing) {
      ctx.db.reaction.id.delete(existing.id);
    } else {
      ctx.db.reaction.insert({ id: 0n, messageId, userId: ctx.sender, emoji });
    }
  }
);

// ─── Permissions ──────────────────────────────────────────────────────────────

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    const adminMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!adminMember?.isAdmin) throw new SenderError('Not an admin');
    const targetMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(userId));
    if (!targetMember) throw new SenderError('User not in room');
    if (targetMember.isAdmin) throw new SenderError('Cannot kick an admin');
    ctx.db.roomMember.id.delete(targetMember.id);
    const typing = [...ctx.db.typingIndicator.roomId.filter(roomId)]
      .find(ti => ti.userId.equals(userId));
    if (typing) ctx.db.typingIndicator.id.delete(typing.id);
  }
);

export const banUser = spacetimedb.reducer(
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    const adminMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!adminMember?.isAdmin) throw new SenderError('Not an admin');
    const targetMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(userId));
    if (targetMember) {
      if (targetMember.isAdmin) throw new SenderError('Cannot ban an admin');
      ctx.db.roomMember.id.update({ ...targetMember, isBanned: true });
    } else {
      ctx.db.roomMember.insert({ id: 0n, roomId, userId, isAdmin: false, isBanned: true });
    }
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    const adminMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(ctx.sender));
    if (!adminMember?.isAdmin) throw new SenderError('Not an admin');
    const targetMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(m => m.userId.equals(userId));
    if (!targetMember) throw new SenderError('User not in room');
    ctx.db.roomMember.id.update({ ...targetMember, isAdmin: true });
  }
);

// ─── Scheduled reducers ───────────────────────────────────────────────────────

export const deliverScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessageDelivery.rowType },
  (ctx, { timer }) => {
    const member = [...ctx.db.roomMember.roomId.filter(timer.roomId)]
      .find(m => m.userId.equals(timer.sender));
    if (member && !member.isBanned) {
      ctx.db.message.insert({
        id: 0n, roomId: timer.roomId, sender: timer.sender, text: timer.text,
        sentAtMicros: ctx.timestamp.microsSinceUnixEpoch,
        isEdited: false, isEphemeral: false, expiresAtMicros: 0n,
      });
    }
  }
);

export const deleteEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralDeleteJob.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.id.find(timer.messageId);
    if (msg) ctx.db.message.id.delete(timer.messageId);
  }
);

export const cleanupTypingIndicators = spacetimedb.reducer(
  { timer: typingCleanupTimer.rowType },
  (ctx, { timer: _timer }) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;
    const expired = [...ctx.db.typingIndicator.iter()].filter(ti => ti.expiresAtMicros <= now);
    for (const ti of expired) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
);

export const checkAway = spacetimedb.reducer(
  { timer: awayChecker.rowType },
  (ctx, { timer: _timer }) => {
    const fiveMinutesMicros = 5n * 60n * 1_000_000n;
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const u of [...ctx.db.user.iter()]) {
      if (u.online && u.status === 'online') {
        if (now - u.lastActiveMicros > fiveMinutesMicros) {
          ctx.db.user.identity.update({ ...u, status: 'away' });
        }
      }
    }
  }
);
