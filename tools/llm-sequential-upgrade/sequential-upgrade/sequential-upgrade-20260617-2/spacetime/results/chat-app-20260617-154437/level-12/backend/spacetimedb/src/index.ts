import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import spacetimedb from './schema';

export { default } from './schema';
export { sendScheduledMessage, deleteExpiredMessage } from './schema';

export const onConnect = spacetimedb.clientConnected((ctx) => {
  if (ctx.connectionId) {
    ctx.db.activeConnection.insert({ connectionId: ctx.connectionId, userIdentity: ctx.sender });
  }
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true, status: 'online', lastActiveAt: ctx.timestamp });
  } else {
    const guestId = ctx.sender.toHexString().slice(0, 8).toUpperCase();
    ctx.db.user.insert({ identity: ctx.sender, name: `Guest-${guestId}`, online: true, status: 'online', lastActiveAt: ctx.timestamp, isAnonymous: true });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  if (ctx.connectionId) {
    ctx.db.activeConnection.connectionId.delete(ctx.connectionId);
  }
  const remaining = [...ctx.db.activeConnection.byIdentity.filter(ctx.sender)];
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    if (remaining.length === 0) {
      ctx.db.user.identity.update({ ...existing, online: false, lastActiveAt: ctx.timestamp });
    } else {
      ctx.db.user.identity.update({ ...existing, lastActiveAt: ctx.timestamp });
    }
  }
  for (const ti of [...ctx.db.typingIndicator.iter()]) {
    if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
});

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, status, lastActiveAt: ctx.timestamp });
  }
);

export const setName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('Not connected');
    ctx.db.user.identity.update({ ...user, name: trimmed, isAnonymous: false });
  }
);

export const createRoom = spacetimedb.reducer(
  { name: t.string(), isPrivate: t.bool() },
  (ctx, { name, isPrivate }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Room name too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const duplicate = [...ctx.db.room.iter()].find(r => r.name === trimmed && !r.isDm);
    if (duplicate) throw new SenderError('Room name already taken');
    const room = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate, isDm: false });
    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
    ctx.db.roomPermission.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender, isAdmin: true, isBanned: false });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    if (room.isPrivate || room.isDm) throw new SenderError('This is a private room. You must be invited.');
    const myPerm = [...ctx.db.roomPermission.byRoom.filter(roomId)]
      .find(p => p.userIdentity.toHexString() === ctx.sender.toHexString());
    if (myPerm?.isBanned) throw new SenderError('You are banned from this room');
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
    ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAtUs: undefined, editedAt: undefined, parentId: undefined });
    const newStatus = user.status === 'away' ? 'online' : user.status;
    ctx.db.user.identity.update({ ...user, status: newStatus, lastActiveAt: ctx.timestamp });
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

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtUs: t.u64() },
  (ctx, { roomId, text, scheduledAtUs }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    if (scheduledAtUs <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }
    ctx.db.scheduledMessage.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(scheduledAtUs),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const scheduled = ctx.db.scheduledMessage.scheduled_id.find(scheduledId);
    if (!scheduled) throw new SenderError('Scheduled message not found');
    if (scheduled.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Cannot cancel another user\'s scheduled message');
    }
    ctx.db.scheduledMessage.scheduled_id.delete(scheduledId);
  }
);

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');
    const existing = [...ctx.db.messageReaction.messageId.filter(messageId)]
      .find(r => r.userIdentity.toHexString() === ctx.sender.toHexString() && r.emoji === emoji);
    if (existing) {
      ctx.db.messageReaction.id.delete(existing.id);
    } else {
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (msg.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Cannot edit another user\'s message');
    }
    ctx.db.messageEdit.insert({ id: 0n, messageId, editedAt: ctx.timestamp, previousText: msg.text });
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const callerPerm = [...ctx.db.roomPermission.byRoom.filter(roomId)]
      .find(p => p.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!callerPerm?.isAdmin) throw new SenderError('Not an admin');
    const targetPerm = [...ctx.db.roomPermission.byRoom.filter(roomId)]
      .find(p => p.userIdentity.toHexString() === targetIdentity.toHexString());
    if (targetPerm?.isAdmin) throw new SenderError('Cannot kick an admin');
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === targetIdentity.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
      }
    }
    if (targetPerm) {
      ctx.db.roomPermission.id.update({ ...targetPerm, isBanned: true });
    } else {
      ctx.db.roomPermission.insert({ id: 0n, roomId, userIdentity: targetIdentity, isAdmin: false, isBanned: true });
    }
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === targetIdentity.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

export const replyToMessage = spacetimedb.reducer(
  { parentMessageId: t.u64(), text: t.string() },
  (ctx, { parentMessageId, text }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Reply cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Reply too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const parent = ctx.db.message.id.find(parentMessageId);
    if (!parent) throw new SenderError('Parent message not found');
    const isMember = [...ctx.db.roomMember.roomId.filter(parent.roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const myPerm = [...ctx.db.roomPermission.byRoom.filter(parent.roomId)]
      .find(p => p.userIdentity.toHexString() === ctx.sender.toHexString());
    if (myPerm?.isBanned) throw new SenderError('You are banned from this room');
    ctx.db.message.insert({
      id: 0n,
      roomId: parent.roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAtUs: undefined,
      editedAt: undefined,
      parentId: parentMessageId,
    });
    const newStatus = user.status === 'away' ? 'online' : user.status;
    ctx.db.user.identity.update({ ...user, status: newStatus, lastActiveAt: ctx.timestamp });
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const callerPerm = [...ctx.db.roomPermission.byRoom.filter(roomId)]
      .find(p => p.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!callerPerm?.isAdmin) throw new SenderError('Not an admin');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === targetIdentity.toHexString());
    if (!isMember) throw new SenderError('User is not a member of this room');
    const existing = [...ctx.db.roomPermission.byRoom.filter(roomId)]
      .find(p => p.userIdentity.toHexString() === targetIdentity.toHexString());
    if (existing) {
      ctx.db.roomPermission.id.update({ ...existing, isAdmin: true, isBanned: false });
    } else {
      ctx.db.roomPermission.insert({ id: 0n, roomId, userIdentity: targetIdentity, isAdmin: true, isBanned: false });
    }
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), ttlSeconds: t.u32() },
  (ctx, { roomId, text, ttlSeconds }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('Not a member of this room');
    const ttlUs = BigInt(ttlSeconds) * 1_000_000n;
    const expiresAtUs = ctx.timestamp.microsSinceUnixEpoch + ttlUs;
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAtUs,
      editedAt: undefined,
      parentId: undefined,
    });
    ctx.db.messageExpiry.insert({
      scheduled_id: 0n,
      scheduled_at: ScheduleAt.time(expiresAtUs),
      messageId: msg.id,
    });
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

export const inviteToRoom = spacetimedb.reducer(
  { roomId: t.u64(), inviteeIdentity: t.identity() },
  (ctx, { roomId, inviteeIdentity }) => {
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!isMember) throw new SenderError('You are not a member of this room');
    const invitee = ctx.db.user.identity.find(inviteeIdentity);
    if (!invitee || !invitee.name) throw new SenderError('User not found');
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(m => m.userIdentity.toHexString() === inviteeIdentity.toHexString());
    if (alreadyMember) throw new SenderError('User is already a member');
    const existingInvite = [...ctx.db.roomInvitation.roomId.filter(roomId)]
      .find(inv => inv.inviteeIdentity.toHexString() === inviteeIdentity.toHexString() && inv.status === 'pending');
    if (existingInvite) throw new SenderError('User already has a pending invitation');
    ctx.db.roomInvitation.insert({
      id: 0n,
      roomId,
      inviterIdentity: ctx.sender,
      inviteeIdentity,
      invitedAt: ctx.timestamp,
      status: 'pending',
    });
  }
);

export const acceptInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (inv.inviteeIdentity.toHexString() !== ctx.sender.toHexString()) throw new SenderError('Not your invitation');
    if (inv.status !== 'pending') throw new SenderError('Invitation is no longer pending');
    ctx.db.roomInvitation.id.update({ ...inv, status: 'accepted' });
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(inv.roomId)]
      .some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
    if (!alreadyMember) {
      ctx.db.roomMember.insert({ id: 0n, roomId: inv.roomId, userIdentity: ctx.sender });
    }
  }
);

export const declineInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (inv.inviteeIdentity.toHexString() !== ctx.sender.toHexString()) throw new SenderError('Not your invitation');
    if (inv.status !== 'pending') throw new SenderError('Invitation is no longer pending');
    ctx.db.roomInvitation.id.update({ ...inv, status: 'declined' });
  }
);

export const saveDraft = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    const existing = [...ctx.db.messageDraft.byUserRoom.filter([ctx.sender, roomId])][0];
    if (text.length === 0) {
      if (existing) ctx.db.messageDraft.id.delete(existing.id);
    } else if (existing) {
      ctx.db.messageDraft.id.update({ ...existing, text, updatedAt: ctx.timestamp });
    } else {
      ctx.db.messageDraft.insert({ id: 0n, userIdentity: ctx.sender, roomId, text, updatedAt: ctx.timestamp });
    }
  }
);

export const createDm = spacetimedb.reducer(
  { targetIdentity: t.identity() },
  (ctx, { targetIdentity }) => {
    if (targetIdentity.toHexString() === ctx.sender.toHexString()) throw new SenderError('Cannot DM yourself');
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');
    const target = ctx.db.user.identity.find(targetIdentity);
    if (!target || !target.name) throw new SenderError('Target user not found');
    // Check if DM already exists between these two users
    for (const r of [...ctx.db.room.iter()]) {
      if (!r.isDm) continue;
      const members = [...ctx.db.roomMember.roomId.filter(r.id)];
      const hasSender = members.some(m => m.userIdentity.toHexString() === ctx.sender.toHexString());
      const hasTarget = members.some(m => m.userIdentity.toHexString() === targetIdentity.toHexString());
      if (hasSender && hasTarget) return; // DM already exists
    }
    const room = ctx.db.room.insert({
      id: 0n,
      name: `dm-${ctx.sender.toHexString().slice(0, 8)}-${targetIdentity.toHexString().slice(0, 8)}`,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
      isPrivate: true,
      isDm: true,
    });
    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userIdentity: ctx.sender });
    ctx.db.roomMember.insert({ id: 0n, roomId: room.id, userIdentity: targetIdentity });
  }
);
