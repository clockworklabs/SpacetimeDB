import { schema, SenderError, t } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  User,
  Room,
  RoomMember,
  Message,
  TypingIndicator,
  ReadReceipt,
  Reaction,
  EditHistory,
  RoomInvitation,
  ScheduledMessage,
  EphemeralCleanup,
  TypingCleanup,
} from './schema';

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  Message,
  TypingIndicator,
  ReadReceipt,
  Reaction,
  EditHistory,
  RoomInvitation,
  ScheduledMessage,
  EphemeralCleanup,
  TypingCleanup
);

// ==================== LIFECYCLE ====================

spacetimedb.clientConnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: true,
      status: 'online',
      lastActive: ctx.timestamp,
    });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: undefined,
      status: 'online',
      lastActive: ctx.timestamp,
      online: true,
    });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: false,
      lastActive: ctx.timestamp,
    });
  }
  
  // Clean up typing indicators for this user
  for (const typing of ctx.db.typingIndicator.iter()) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
    }
  }
});

// ==================== USER REDUCERS ====================

spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  const trimmed = name.trim();
  if (!trimmed || trimmed.length > 50) {
    throw new SenderError('Name must be 1-50 characters');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');
  
  ctx.db.user.identity.update({ ...user, name: trimmed, lastActive: ctx.timestamp });
});

spacetimedb.reducer('set_status', { status: t.string() }, (ctx, { status }) => {
  const validStatuses = ['online', 'away', 'dnd', 'invisible'];
  if (!validStatuses.includes(status)) {
    throw new SenderError('Invalid status');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');
  
  ctx.db.user.identity.update({ ...user, status, lastActive: ctx.timestamp });
});

spacetimedb.reducer('heartbeat', {}, (ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, lastActive: ctx.timestamp });
  }
});

// ==================== ROOM REDUCERS ====================

spacetimedb.reducer('create_room', { name: t.string(), isPrivate: t.bool() }, (ctx, { name, isPrivate }) => {
  const trimmed = name.trim();
  if (!trimmed || trimmed.length > 100) {
    throw new SenderError('Room name must be 1-100 characters');
  }
  
  const roomId = ctx.db.room.insert({
    id: 0n,
    name: trimmed,
    creatorId: ctx.sender,
    isPrivate,
    isDm: false,
    createdAt: ctx.timestamp,
  }).id;
  
  // Creator is automatically admin
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    role: 'admin',
    joinedAt: ctx.timestamp,
  });
});

spacetimedb.reducer('join_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  if (room.isPrivate) {
    throw new SenderError('Cannot join private room without invitation');
  }
  
  // Check if already member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Already a member');
    }
  }
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    role: 'member',
    joinedAt: ctx.timestamp,
  });
});

spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  
  throw new SenderError('Not a member');
});

// ==================== PRIVATE ROOM / DM REDUCERS ====================

spacetimedb.reducer('create_dm', { targetUserIdentity: t.string() }, (ctx, { targetUserIdentity }) => {
  // Find target user
  let targetUser = null;
  for (const u of ctx.db.user.iter()) {
    if (u.identity.toHexString() === targetUserIdentity) {
      targetUser = u;
      break;
    }
  }
  if (!targetUser) throw new SenderError('User not found');
  
  // Check if DM already exists between these users
  for (const room of ctx.db.room.iter()) {
    if (room.isDm) {
      const members = [...ctx.db.roomMember.by_room.filter(room.id)];
      const memberIds = members.map(m => m.userId.toHexString());
      if (memberIds.length === 2 &&
          memberIds.includes(ctx.sender.toHexString()) &&
          memberIds.includes(targetUserIdentity)) {
        throw new SenderError('DM already exists');
      }
    }
  }
  
  const roomId = ctx.db.room.insert({
    id: 0n,
    name: 'DM',
    creatorId: ctx.sender,
    isPrivate: true,
    isDm: true,
    createdAt: ctx.timestamp,
  }).id;
  
  // Add both users as members
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    role: 'admin',
    joinedAt: ctx.timestamp,
  });
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userId: targetUser.identity,
    role: 'admin',
    joinedAt: ctx.timestamp,
  });
});

spacetimedb.reducer('invite_to_room', { roomId: t.u64(), inviteeIdentity: t.string() }, (ctx, { roomId, inviteeIdentity }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  if (!room.isPrivate) throw new SenderError('Room is not private');
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.role === 'admin') {
      isAdmin = true;
      break;
    }
  }
  if (!isAdmin) throw new SenderError('Only admins can invite');
  
  // Check if target user exists
  let targetUser = null;
  for (const u of ctx.db.user.iter()) {
    if (u.identity.toHexString() === inviteeIdentity) {
      targetUser = u;
      break;
    }
  }
  if (!targetUser) throw new SenderError('User not found');
  
  // Check if already member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === inviteeIdentity) {
      throw new SenderError('User is already a member');
    }
  }
  
  // Check if invitation already exists
  for (const inv of ctx.db.roomInvitation.by_invitee.filter(targetUser.identity)) {
    if (inv.roomId === roomId && inv.status === 'pending') {
      throw new SenderError('Invitation already pending');
    }
  }
  
  ctx.db.roomInvitation.insert({
    id: 0n,
    roomId,
    inviterId: ctx.sender,
    inviteeId: targetUser.identity,
    status: 'pending',
    createdAt: ctx.timestamp,
  });
});

spacetimedb.reducer('respond_to_invitation', { invitationId: t.u64(), accept: t.bool() }, (ctx, { invitationId, accept }) => {
  const invitation = ctx.db.roomInvitation.id.find(invitationId);
  if (!invitation) throw new SenderError('Invitation not found');
  if (invitation.inviteeId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Not your invitation');
  }
  if (invitation.status !== 'pending') {
    throw new SenderError('Invitation already responded');
  }
  
  ctx.db.roomInvitation.id.update({
    ...invitation,
    status: accept ? 'accepted' : 'declined',
  });
  
  if (accept) {
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: invitation.roomId,
      userId: ctx.sender,
      role: 'member',
      joinedAt: ctx.timestamp,
    });
  }
});

// ==================== MESSAGE REDUCERS ====================

spacetimedb.reducer('send_message', { roomId: t.u64(), content: t.string(), parentId: t.u64().optional() }, (ctx, { roomId, content, parentId }) => {
  const trimmed = content.trim();
  if (!trimmed || trimmed.length > 2000) {
    throw new SenderError('Message must be 1-2000 characters');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this room');
  
  // Validate parent if threading
  if (parentId != null) {
    const parent = ctx.db.message.id.find(parentId);
    if (!parent || parent.roomId !== roomId) {
      throw new SenderError('Invalid parent message');
    }
  }
  
  ctx.db.message.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    content: trimmed,
    createdAt: ctx.timestamp,
    editedAt: undefined,
    parentId,
    expiresAt: undefined,
  });
  
  // Clear typing indicator
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
    }
  }
});

spacetimedb.reducer('send_ephemeral_message', { roomId: t.u64(), content: t.string(), durationSeconds: t.u64() }, (ctx, { roomId, content, durationSeconds }) => {
  const trimmed = content.trim();
  if (!trimmed || trimmed.length > 2000) {
    throw new SenderError('Message must be 1-2000 characters');
  }
  if (durationSeconds < 10n || durationSeconds > 3600n) {
    throw new SenderError('Duration must be 10-3600 seconds');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this room');
  
  const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + durationSeconds * 1_000_000n;
  const expiresAt = new Timestamp(expiresAtMicros);
  
  const message = ctx.db.message.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    content: trimmed,
    createdAt: ctx.timestamp,
    editedAt: undefined,
    parentId: undefined,
    expiresAt,
  });
  
  // Schedule cleanup
  ctx.db.ephemeralCleanup.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(expiresAtMicros),
    messageId: message.id,
  });
});

spacetimedb.reducer('edit_message', { messageId: t.u64(), newContent: t.string() }, (ctx, { messageId, newContent }) => {
  const trimmed = newContent.trim();
  if (!trimmed || trimmed.length > 2000) {
    throw new SenderError('Message must be 1-2000 characters');
  }
  
  const message = ctx.db.message.id.find(messageId);
  if (!message) throw new SenderError('Message not found');
  if (message.senderId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Can only edit your own messages');
  }
  
  // Save edit history
  ctx.db.editHistory.insert({
    id: 0n,
    messageId,
    oldContent: message.content,
    editedAt: ctx.timestamp,
  });
  
  ctx.db.message.id.update({
    ...message,
    content: trimmed,
    editedAt: ctx.timestamp,
  });
});

spacetimedb.reducer('delete_message', { messageId: t.u64() }, (ctx, { messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) throw new SenderError('Message not found');
  if (message.senderId.toHexString() !== ctx.sender.toHexString()) {
    // Check if admin in room
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString() && member.role === 'admin') {
        isAdmin = true;
        break;
      }
    }
    if (!isAdmin) throw new SenderError('Can only delete your own messages');
  }
  
  // Delete reactions
  for (const reaction of ctx.db.reaction.by_message.filter(messageId)) {
    ctx.db.reaction.id.delete(reaction.id);
  }
  
  // Delete edit history
  for (const edit of ctx.db.editHistory.by_message.filter(messageId)) {
    ctx.db.editHistory.id.delete(edit.id);
  }
  
  ctx.db.message.id.delete(messageId);
});

// ==================== SCHEDULED MESSAGE REDUCERS ====================

spacetimedb.reducer('schedule_message', { roomId: t.u64(), content: t.string(), sendAtMicros: t.u64() }, (ctx, { roomId, content, sendAtMicros }) => {
  const trimmed = content.trim();
  if (!trimmed || trimmed.length > 2000) {
    throw new SenderError('Message must be 1-2000 characters');
  }
  
  if (sendAtMicros <= ctx.timestamp.microsSinceUnixEpoch) {
    throw new SenderError('Scheduled time must be in the future');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this room');
  
  ctx.db.scheduledMessage.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(sendAtMicros),
    roomId,
    senderId: ctx.sender,
    content: trimmed,
  });
});

spacetimedb.reducer('cancel_scheduled_message', { scheduledId: t.u64() }, (ctx, { scheduledId }) => {
  const scheduled = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
  if (!scheduled) throw new SenderError('Scheduled message not found');
  if (scheduled.senderId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Can only cancel your own scheduled messages');
  }
  
  ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
});

spacetimedb.reducer('send_scheduled_message', { arg: ScheduledMessage.rowType }, (ctx, { arg }) => {
  // Check if user is still a member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(arg.roomId)) {
    if (member.userId.toHexString() === arg.senderId.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (isMember) {
    ctx.db.message.insert({
      id: 0n,
      roomId: arg.roomId,
      senderId: arg.senderId,
      content: arg.content,
      createdAt: ctx.timestamp,
      editedAt: undefined,
      parentId: undefined,
      expiresAt: undefined,
    });
  }
});

// ==================== EPHEMERAL CLEANUP ====================

spacetimedb.reducer('cleanup_ephemeral_message', { arg: EphemeralCleanup.rowType }, (ctx, { arg }) => {
  const message = ctx.db.message.id.find(arg.messageId);
  if (message) {
    // Delete reactions
    for (const reaction of ctx.db.reaction.by_message.filter(arg.messageId)) {
      ctx.db.reaction.id.delete(reaction.id);
    }
    // Delete edit history
    for (const edit of ctx.db.editHistory.by_message.filter(arg.messageId)) {
      ctx.db.editHistory.id.delete(edit.id);
    }
    ctx.db.message.id.delete(arg.messageId);
  }
});

// ==================== TYPING INDICATORS ====================

spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this room');
  
  // Check if already typing
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      // Update timestamp
      ctx.db.typingIndicator.id.update({ ...typing, startedAt: ctx.timestamp });
      return;
    }
  }
  
  const typingId = ctx.db.typingIndicator.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    startedAt: ctx.timestamp,
  }).id;
  
  // Schedule cleanup after 5 seconds
  const cleanupAt = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
  ctx.db.typingCleanup.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(cleanupAt),
    typingId,
  });
});

spacetimedb.reducer('stop_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
      return;
    }
  }
});

spacetimedb.reducer('cleanup_typing_indicator', { arg: TypingCleanup.rowType }, (ctx, { arg }) => {
  const typing = ctx.db.typingIndicator.id.find(arg.typingId);
  if (typing) {
    // Only delete if older than 4 seconds
    const age = ctx.timestamp.microsSinceUnixEpoch - typing.startedAt.microsSinceUnixEpoch;
    if (age > 4_000_000n) {
      ctx.db.typingIndicator.id.delete(arg.typingId);
    }
  }
});

// ==================== READ RECEIPTS ====================

spacetimedb.reducer('mark_read', { roomId: t.u64(), messageId: t.u64() }, (ctx, { roomId, messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message || message.roomId !== roomId) {
    throw new SenderError('Message not found');
  }
  
  // Find existing read receipt for this user/room
  for (const receipt of ctx.db.readReceipt.by_room.filter(roomId)) {
    if (receipt.userId.toHexString() === ctx.sender.toHexString()) {
      // Only update if this message is newer
      if (messageId > receipt.lastReadMessageId) {
        ctx.db.readReceipt.id.update({
          ...receipt,
          lastReadMessageId: messageId,
          readAt: ctx.timestamp,
        });
      }
      return;
    }
  }
  
  // Create new read receipt
  ctx.db.readReceipt.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    lastReadMessageId: messageId,
    readAt: ctx.timestamp,
  });
});

// ==================== REACTIONS ====================

spacetimedb.reducer('toggle_reaction', { messageId: t.u64(), emoji: t.string() }, (ctx, { messageId, emoji }) => {
  const validEmojis = ['👍', '❤️', '😂', '😮', '😢', '🎉', '🔥', '👀'];
  if (!validEmojis.includes(emoji)) {
    throw new SenderError('Invalid emoji');
  }
  
  const message = ctx.db.message.id.find(messageId);
  if (!message) throw new SenderError('Message not found');
  
  // Check if member of room
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  if (!isMember) throw new SenderError('Not a member of this room');
  
  // Check if reaction exists
  for (const reaction of ctx.db.reaction.by_message.filter(messageId)) {
    if (reaction.userId.toHexString() === ctx.sender.toHexString() && reaction.emoji === emoji) {
      // Remove reaction
      ctx.db.reaction.id.delete(reaction.id);
      return;
    }
  }
  
  // Add reaction
  ctx.db.reaction.insert({
    id: 0n,
    messageId,
    userId: ctx.sender,
    emoji,
  });
});

// ==================== PERMISSIONS ====================

spacetimedb.reducer('promote_to_admin', { roomId: t.u64(), targetIdentity: t.string() }, (ctx, { roomId, targetIdentity }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.role === 'admin') {
      isAdmin = true;
      break;
    }
  }
  if (!isAdmin) throw new SenderError('Only admins can promote users');
  
  // Find target member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === targetIdentity) {
      ctx.db.roomMember.id.update({ ...member, role: 'admin' });
      return;
    }
  }
  
  throw new SenderError('User is not a member of this room');
});

spacetimedb.reducer('kick_user', { roomId: t.u64(), targetIdentity: t.string() }, (ctx, { roomId, targetIdentity }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.role === 'admin') {
      isAdmin = true;
      break;
    }
  }
  if (!isAdmin) throw new SenderError('Only admins can kick users');
  
  // Cannot kick yourself
  if (targetIdentity === ctx.sender.toHexString()) {
    throw new SenderError('Cannot kick yourself');
  }
  
  // Find and remove target member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === targetIdentity) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  
  throw new SenderError('User is not a member of this room');
});

export default spacetimedb;
