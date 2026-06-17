import { SenderError, t } from 'spacetimedb/server';
import spacetimedb, { ScheduleAt } from './schema.js';
export { default, sendScheduledMessage, deleteExpiredMessage, checkPresence } from './schema.js';

const ALLOWED_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

export const init = spacetimedb.init((ctx) => {
  // Start the global presence timer (repeats every 60 seconds for auto-away)
  if ([...ctx.db.presenceTimer.iter()].length === 0) {
    ctx.db.presenceTimer.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.interval(60_000_000n),
    });
  }
});

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    // Restore to online unless user explicitly set DND or invisible
    const newStatus = (existing.status === 'dnd' || existing.status === 'invisible')
      ? existing.status
      : 'online';
    ctx.db.user.identity.update({ ...existing, online: true, status: newStatus, lastActiveAt: ctx.timestamp });
  }
  // Fallback: ensure presence timer is running (in case init didn't fire on re-publish)
  if ([...ctx.db.presenceTimer.iter()].length === 0) {
    ctx.db.presenceTimer.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.interval(60_000_000n),
    });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActiveAt: ctx.timestamp });
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
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true, status: 'online', lastActiveAt: ctx.timestamp });
    }
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Must set name first');
    ctx.db.user.identity.update({ ...existing, status, lastActiveAt: ctx.timestamp });
  }
);

export const createRoom = spacetimedb.reducer(
  { name: t.string(), isPrivate: t.bool() },
  (ctx, { name, isPrivate }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long');

    const room = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate, isDm: false });
    ctx.db.membership.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
    ctx.db.roomAdmin.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const roomRow = ctx.db.room.id.find(roomId);
    if (!roomRow) throw new SenderError('Room not found');

    const banned = [...ctx.db.roomBan.by_room_user.filter([roomId, ctx.sender])];
    if (banned.length > 0) throw new SenderError('You are banned from this room');

    const existing = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (existing.length > 0) return;

    if (roomRow.isPrivate) {
      const inv = [...ctx.db.invitation.by_room_invitee.filter([roomId, ctx.sender])];
      if (!inv.some(i => i.status === 'accepted')) {
        throw new SenderError('This is a private room. You need an accepted invitation to join.');
      }
    }

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

    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAt: undefined, editedAt: undefined, parentMessageId: undefined });

    // Track activity and restore from auto-away
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (userRow) {
      const newStatus = userRow.status === 'away' ? 'online' : userRow.status;
      ctx.db.user.identity.update({ ...userRow, lastActiveAt: ctx.timestamp, status: newStatus });
    }

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
      // Track activity and restore from auto-away when user is actively typing
      const userRow = ctx.db.user.identity.find(ctx.sender);
      if (userRow) {
        const newStatus = userRow.status === 'away' ? 'online' : userRow.status;
        ctx.db.user.identity.update({ ...userRow, lastActiveAt: ctx.timestamp, status: newStatus });
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

// Schedule a message to be sent at a future time (scheduledAtMicros = Unix epoch microseconds)
export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtMicros: t.i64() },
  (ctx, { roomId, text, scheduledAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    ctx.db.scheduledMessage.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(BigInt(scheduledAtMicros)),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

// Cancel a pending scheduled message (only the sender can cancel)
export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const row = ctx.db.scheduledMessage.scheduled_id.find(scheduledId);
    if (!row) throw new SenderError('Scheduled message not found');
    if (!row.senderIdentity.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.scheduled_id.delete(scheduledId);
  }
);

// Toggle a reaction emoji on a message (add if absent, remove if present)
export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    if (!ALLOWED_EMOJIS.includes(emoji)) throw new SenderError('Invalid emoji');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    const existing = [...ctx.db.messageReaction.by_message_user.filter([messageId, ctx.sender])]
      .find(r => r.emoji === emoji);

    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

// Edit a message (only the sender can edit); saves previous text as edit history
export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.senderIdentity.equals(ctx.sender)) throw new SenderError('Can only edit your own messages');

    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');

    // Save current text to edit history before overwriting
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      previousText: msg.text,
      editedAt: ctx.timestamp,
    });

    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// Kick and ban a user from a room (admin only)
export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), userIdentity: t.identity() },
  (ctx, { roomId, userIdentity }) => {
    const isAdmin = [...ctx.db.roomAdmin.by_room_user.filter([roomId, ctx.sender])].length > 0;
    if (!isAdmin) throw new SenderError('Only admins can kick users');
    if (userIdentity.equals(ctx.sender)) throw new SenderError('Cannot kick yourself');

    const targetIsAdmin = [...ctx.db.roomAdmin.by_room_user.filter([roomId, userIdentity])].length > 0;
    if (targetIsAdmin) throw new SenderError('Cannot kick another admin');

    // Remove membership
    for (const m of [...ctx.db.membership.by_room_user.filter([roomId, userIdentity])]) {
      ctx.db.membership.id.delete(m.id);
    }
    // Clear typing indicator
    for (const ti of [...ctx.db.typingIndicator.by_room.filter(roomId)]) {
      if (ti.userIdentity.equals(userIdentity)) ctx.db.typingIndicator.id.delete(ti.id);
    }
    // Add ban
    if ([...ctx.db.roomBan.by_room_user.filter([roomId, userIdentity])].length === 0) {
      ctx.db.roomBan.insert({ id: 0n, roomId, userIdentity });
    }
  }
);

// Reply to a message, creating a thread reply
export const replyToMessage = spacetimedb.reducer(
  { parentMessageId: t.u64(), text: t.string() },
  (ctx, { parentMessageId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Reply cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Reply too long');

    const parent = ctx.db.message.id.find(parentMessageId);
    if (!parent) throw new SenderError('Parent message not found');

    const memberships = [...ctx.db.membership.by_room_user.filter([parent.roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    ctx.db.message.insert({
      id: 0n,
      roomId: parent.roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: undefined,
      editedAt: undefined,
      parentMessageId,
    });

    // Track activity
    const userRow = ctx.db.user.identity.find(ctx.sender);
    if (userRow) {
      const newStatus = userRow.status === 'away' ? 'online' : userRow.status;
      ctx.db.user.identity.update({ ...userRow, lastActiveAt: ctx.timestamp, status: newStatus });
    }
  }
);

// Promote a room member to admin (admin only)
export const promoteToAdmin = spacetimedb.reducer(
  { roomId: t.u64(), userIdentity: t.identity() },
  (ctx, { roomId, userIdentity }) => {
    const isAdmin = [...ctx.db.roomAdmin.by_room_user.filter([roomId, ctx.sender])].length > 0;
    if (!isAdmin) throw new SenderError('Only admins can promote users');

    if ([...ctx.db.membership.by_room_user.filter([roomId, userIdentity])].length === 0) {
      throw new SenderError('User is not a member of this room');
    }
    if ([...ctx.db.roomAdmin.by_room_user.filter([roomId, userIdentity])].length === 0) {
      ctx.db.roomAdmin.insert({ id: 0n, roomId, userIdentity });
    }
  }
);

// Invite a user to a private room (admin only)
export const inviteUser = spacetimedb.reducer(
  { roomId: t.u64(), inviteeIdentity: t.identity() },
  (ctx, { roomId, inviteeIdentity }) => {
    const isAdmin = [...ctx.db.roomAdmin.by_room_user.filter([roomId, ctx.sender])].length > 0;
    if (!isAdmin) throw new SenderError('Only admins can invite users');

    const roomRow = ctx.db.room.id.find(roomId);
    if (!roomRow) throw new SenderError('Room not found');
    if (!roomRow.isPrivate) throw new SenderError('Room is not private');

    if (!ctx.db.user.identity.find(inviteeIdentity)) throw new SenderError('User not found');

    if ([...ctx.db.membership.by_room_user.filter([roomId, inviteeIdentity])].length > 0) {
      throw new SenderError('User is already a member');
    }

    const existing = [...ctx.db.invitation.by_room_invitee.filter([roomId, inviteeIdentity])];
    if (existing.some(i => i.status === 'pending')) throw new SenderError('User is already invited');

    ctx.db.invitation.insert({ id: 0n, roomId, inviterIdentity: ctx.sender, inviteeIdentity, status: 'pending', createdAt: ctx.timestamp });
  }
);

// Accept or decline an invitation
export const respondToInvitation = spacetimedb.reducer(
  { invitationId: t.u64(), accept: t.bool() },
  (ctx, { invitationId, accept }) => {
    const inv = ctx.db.invitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (!inv.inviteeIdentity.equals(ctx.sender)) throw new SenderError('Not your invitation');
    if (inv.status !== 'pending') throw new SenderError('Invitation already responded to');

    ctx.db.invitation.id.update({ ...inv, status: accept ? 'accepted' : 'declined' });

    if (accept) {
      const banned = [...ctx.db.roomBan.by_room_user.filter([inv.roomId, ctx.sender])];
      if (banned.length > 0) throw new SenderError('You are banned from this room');
      if ([...ctx.db.membership.by_room_user.filter([inv.roomId, ctx.sender])].length === 0) {
        ctx.db.membership.insert({ id: 0n, roomId: inv.roomId, userIdentity: ctx.sender });
      }
    }
  }
);

// Create or find a direct message room between the caller and another user
export const startDm = spacetimedb.reducer(
  { otherIdentity: t.identity() },
  (ctx, { otherIdentity }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    if (!ctx.db.user.identity.find(otherIdentity)) throw new SenderError('User not found');
    if (otherIdentity.equals(ctx.sender)) throw new SenderError('Cannot DM yourself');

    const hexA = ctx.sender.toHexString();
    const hexB = otherIdentity.toHexString();
    const [first, second] = hexA < hexB ? [hexA, hexB] : [hexB, hexA];
    const dmName = `__dm__${first}__${second}`;

    const existingRoom = ctx.db.room.name.find(dmName);
    if (existingRoom) {
      if ([...ctx.db.membership.by_room_user.filter([existingRoom.id, ctx.sender])].length === 0) {
        ctx.db.membership.insert({ id: 0n, roomId: existingRoom.id, userIdentity: ctx.sender });
      }
      return;
    }

    const dmRoom = ctx.db.room.insert({ id: 0n, name: dmName, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate: true, isDm: true });
    ctx.db.membership.insert({ id: 0n, roomId: dmRoom.id, userIdentity: ctx.sender });
    ctx.db.membership.insert({ id: 0n, roomId: dmRoom.id, userIdentity: otherIdentity });
  }
);

// Save or clear a message draft for the given room (text='' deletes the draft)
export const saveDraft = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    if (text.length > 2000) throw new SenderError('Draft too long');

    const existing = [...ctx.db.messageDraft.by_room_user.filter([roomId, ctx.sender])];
    if (text.length === 0) {
      for (const d of existing) ctx.db.messageDraft.id.delete(d.id);
      return;
    }
    if (existing.length > 0) {
      ctx.db.messageDraft.id.update({ ...existing[0], text, updatedAt: ctx.timestamp });
    } else {
      ctx.db.messageDraft.insert({ id: 0n, roomId, userIdentity: ctx.sender, text, updatedAt: ctx.timestamp });
    }
  }
);

// Send an ephemeral message that auto-deletes after expirySeconds
export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), expirySeconds: t.u32() },
  (ctx, { roomId, text, expirySeconds }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Must set name first');
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    if (expirySeconds < 1 || expirySeconds > 3600) throw new SenderError('Invalid expiry duration');

    const memberships = [...ctx.db.membership.by_room_user.filter([roomId, ctx.sender])];
    if (memberships.length === 0) throw new SenderError('Must join room first');

    const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + BigInt(expirySeconds) * 1_000_000n;

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: expiresAtMicros,
      editedAt: undefined,
      parentMessageId: undefined,
    });

    ctx.db.messageExpiry.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(expiresAtMicros),
      messageId: msg.id,
    });

    // Clear typing indicator
    for (const indicator of [...ctx.db.typingIndicator.by_room.filter(roomId)]) {
      if (indicator.userIdentity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }
  }
);
