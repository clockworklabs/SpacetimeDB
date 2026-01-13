import { SenderError, t } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  spacetimedb,
  User,
  Room,
  RoomMember,
  RoomInvitation,
  Message,
  MessageEdit,
  Reaction,
  ReadReceipt,
  TypingIndicator,
  ScheduledMessage,
  EphemeralMessageCleanup,
  AwayStatusJob,
} from './schema';

// ============================================================================
// CONSTANTS
// ============================================================================

const TYPING_EXPIRY_MS = 5000n; // 5 seconds
const AWAY_CHECK_MS = 300000n; // 5 minutes
const AWAY_THRESHOLD_MS = 300000000n; // 5 minutes in microseconds

// ============================================================================
// LIFECYCLE REDUCERS
// ============================================================================

spacetimedb.clientConnected((ctx) => {
  const existingUser = ctx.db.user.identity.find(ctx.sender);
  if (existingUser) {
    ctx.db.user.identity.update({
      ...existingUser,
      online: true,
      status: existingUser.status === 'invisible' ? 'invisible' : 'online',
      lastActive: ctx.timestamp,
      connectionId: ctx.connectionId?.value,
    });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: undefined,
      online: true,
      status: 'online',
      lastActive: ctx.timestamp,
      connectionId: ctx.connectionId?.value,
    });
  }
  
  // Schedule away check
  scheduleAwayCheck(ctx, ctx.sender);
});

spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: false,
      lastActive: ctx.timestamp,
      connectionId: undefined,
    });
  }
  
  // Clean up typing indicators
  for (const indicator of ctx.db.typingIndicator.iter()) {
    if (indicator.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.scheduledId.delete(indicator.scheduledId);
    }
  }
  
  // Cancel away check jobs
  for (const job of [...ctx.db.awayStatusJob.by_user.filter(ctx.sender)]) {
    ctx.db.awayStatusJob.scheduledId.delete(job.scheduledId);
  }
});

function scheduleAwayCheck(ctx: any, userIdentity: any): void {
  // Cancel existing away check jobs for this user
  for (const job of [...ctx.db.awayStatusJob.by_user.filter(userIdentity)]) {
    ctx.db.awayStatusJob.scheduledId.delete(job.scheduledId);
  }
  
  // Schedule new check
  const scheduledAt = ScheduleAt.interval(AWAY_CHECK_MS * 1000n); // Convert to micros
  ctx.db.awayStatusJob.insert({
    scheduledId: 0n,
    scheduledAt,
    userIdentity,
  });
}

// ============================================================================
// USER REDUCERS
// ============================================================================

spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    throw new SenderError('Name cannot be empty');
  }
  if (trimmed.length > 50) {
    throw new SenderError('Name too long (max 50 characters)');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('User not found');
  }
  
  ctx.db.user.identity.update({ ...user, name: trimmed, lastActive: ctx.timestamp });
  updateActivity(ctx);
});

spacetimedb.reducer('set_status', { status: t.string() }, (ctx, { status }) => {
  const validStatuses = ['online', 'away', 'dnd', 'invisible'];
  if (!validStatuses.includes(status)) {
    throw new SenderError('Invalid status');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('User not found');
  }
  
  ctx.db.user.identity.update({ ...user, status, lastActive: ctx.timestamp });
  updateActivity(ctx);
});

function updateActivity(ctx: any): void {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user && user.status === 'away') {
    ctx.db.user.identity.update({ ...user, status: 'online', lastActive: ctx.timestamp });
  }
  scheduleAwayCheck(ctx, ctx.sender);
}

// Scheduled reducer for checking away status
spacetimedb.reducer('check_away_status', { arg: AwayStatusJob.rowType }, (ctx, { arg }) => {
  const user = ctx.db.user.identity.find(arg.userIdentity);
  if (!user || !user.online) return;
  
  const timeSinceActive = ctx.timestamp.microsSinceUnixEpoch - user.lastActive.microsSinceUnixEpoch;
  
  if (user.status === 'online' && timeSinceActive > AWAY_THRESHOLD_MS) {
    ctx.db.user.identity.update({ ...user, status: 'away' });
  }
  
  // Reschedule if user still online
  if (user.online) {
    scheduleAwayCheck(ctx, arg.userIdentity);
  }
});

// ============================================================================
// ROOM REDUCERS
// ============================================================================

spacetimedb.reducer('create_room', { name: t.string(), isPrivate: t.bool() }, (ctx, { name, isPrivate }) => {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    throw new SenderError('Room name cannot be empty');
  }
  if (trimmed.length > 100) {
    throw new SenderError('Room name too long');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) {
    throw new SenderError('You must set a name first');
  }
  
  const room = ctx.db.room.insert({
    id: 0n,
    name: trimmed,
    creatorIdentity: ctx.sender,
    createdAt: ctx.timestamp,
    isPrivate,
    isDm: false,
  });
  
  // Creator auto-joins as admin
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: room.id,
    userIdentity: ctx.sender,
    isAdmin: true,
    isBanned: false,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('join_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  if (room.isPrivate) {
    throw new SenderError('This room is private. You need an invitation.');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) {
    throw new SenderError('You must set a name first');
  }
  
  // Check if already a member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      if (member.isBanned) {
        throw new SenderError('You are banned from this room');
      }
      throw new SenderError('Already a member of this room');
    }
  }
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userIdentity: ctx.sender,
    isAdmin: false,
    isBanned: false,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  
  throw new SenderError('Not a member of this room');
});

// ============================================================================
// PRIVATE ROOM / DM REDUCERS
// ============================================================================

spacetimedb.reducer('invite_to_room', { roomId: t.u64(), inviteeUsername: t.string() }, (ctx, { roomId, inviteeUsername }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      isAdmin = member.isAdmin;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can invite users');
  }
  
  // Find invitee by username
  let invitee = null;
  for (const user of ctx.db.user.iter()) {
    if (user.name === inviteeUsername.trim()) {
      invitee = user;
      break;
    }
  }
  
  if (!invitee) {
    throw new SenderError('User not found');
  }
  
  // Check if already a member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === invitee.identity.toHexString()) {
      throw new SenderError('User is already a member');
    }
  }
  
  // Check for existing pending invitation
  for (const inv of ctx.db.roomInvitation.by_invitee.filter(invitee.identity)) {
    if (inv.roomId === roomId && inv.status === 'pending') {
      throw new SenderError('Invitation already pending');
    }
  }
  
  ctx.db.roomInvitation.insert({
    id: 0n,
    roomId,
    inviterIdentity: ctx.sender,
    inviteeIdentity: invitee.identity,
    createdAt: ctx.timestamp,
    status: 'pending',
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('respond_to_invitation', { invitationId: t.u64(), accept: t.bool() }, (ctx, { invitationId, accept }) => {
  const invitation = ctx.db.roomInvitation.id.find(invitationId);
  if (!invitation) {
    throw new SenderError('Invitation not found');
  }
  
  if (invitation.inviteeIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('This invitation is not for you');
  }
  
  if (invitation.status !== 'pending') {
    throw new SenderError('Invitation already responded to');
  }
  
  ctx.db.roomInvitation.id.update({ ...invitation, status: accept ? 'accepted' : 'declined' });
  
  if (accept) {
    const room = ctx.db.room.id.find(invitation.roomId);
    if (!room) {
      throw new SenderError('Room no longer exists');
    }
    
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: invitation.roomId,
      userIdentity: ctx.sender,
      isAdmin: false,
      isBanned: false,
      joinedAt: ctx.timestamp,
      lastReadMessageId: undefined,
    });
  }
  
  updateActivity(ctx);
});

spacetimedb.reducer('start_dm', { targetUsername: t.string() }, (ctx, { targetUsername }) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) {
    throw new SenderError('You must set a name first');
  }
  
  // Find target user
  let target = null;
  for (const u of ctx.db.user.iter()) {
    if (u.name === targetUsername.trim()) {
      target = u;
      break;
    }
  }
  
  if (!target) {
    throw new SenderError('User not found');
  }
  
  if (target.identity.toHexString() === ctx.sender.toHexString()) {
    throw new SenderError('Cannot DM yourself');
  }
  
  // Check if DM already exists between these users
  for (const room of ctx.db.room.iter()) {
    if (room.isDm) {
      const members = [...ctx.db.roomMember.by_room.filter(room.id)];
      if (members.length === 2) {
        const identities = members.map(m => m.userIdentity.toHexString());
        if (identities.includes(ctx.sender.toHexString()) && identities.includes(target.identity.toHexString())) {
          throw new SenderError('DM already exists with this user');
        }
      }
    }
  }
  
  // Create DM room
  const dmRoom = ctx.db.room.insert({
    id: 0n,
    name: `DM: ${user.name} & ${target.name}`,
    creatorIdentity: ctx.sender,
    createdAt: ctx.timestamp,
    isPrivate: true,
    isDm: true,
  });
  
  // Add both users as members
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: dmRoom.id,
    userIdentity: ctx.sender,
    isAdmin: true,
    isBanned: false,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
  });
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: dmRoom.id,
    userIdentity: target.identity,
    isAdmin: true,
    isBanned: false,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
  });
  
  updateActivity(ctx);
});

// ============================================================================
// PERMISSION REDUCERS
// ============================================================================

spacetimedb.reducer('kick_user', { roomId: t.u64(), targetUsername: t.string() }, (ctx, { roomId, targetUsername }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let senderMember = null;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      senderMember = member;
      break;
    }
  }
  
  if (!senderMember || !senderMember.isAdmin) {
    throw new SenderError('Only admins can kick users');
  }
  
  // Find target user
  let target = null;
  for (const u of ctx.db.user.iter()) {
    if (u.name === targetUsername.trim()) {
      target = u;
      break;
    }
  }
  
  if (!target) {
    throw new SenderError('User not found');
  }
  
  // Find and remove target's membership
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === target.identity.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  
  throw new SenderError('User is not a member of this room');
});

spacetimedb.reducer('ban_user', { roomId: t.u64(), targetUsername: t.string() }, (ctx, { roomId, targetUsername }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let senderMember = null;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      senderMember = member;
      break;
    }
  }
  
  if (!senderMember || !senderMember.isAdmin) {
    throw new SenderError('Only admins can ban users');
  }
  
  // Find target user
  let target = null;
  for (const u of ctx.db.user.iter()) {
    if (u.name === targetUsername.trim()) {
      target = u;
      break;
    }
  }
  
  if (!target) {
    throw new SenderError('User not found');
  }
  
  // Find and ban target
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === target.identity.toHexString()) {
      ctx.db.roomMember.id.update({ ...member, isBanned: true });
      return;
    }
  }
  
  throw new SenderError('User is not a member of this room');
});

spacetimedb.reducer('promote_to_admin', { roomId: t.u64(), targetUsername: t.string() }, (ctx, { roomId, targetUsername }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let senderMember = null;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString()) {
      senderMember = member;
      break;
    }
  }
  
  if (!senderMember || !senderMember.isAdmin) {
    throw new SenderError('Only admins can promote users');
  }
  
  // Find target user
  let target = null;
  for (const u of ctx.db.user.iter()) {
    if (u.name === targetUsername.trim()) {
      target = u;
      break;
    }
  }
  
  if (!target) {
    throw new SenderError('User not found');
  }
  
  // Find and promote target
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === target.identity.toHexString()) {
      ctx.db.roomMember.id.update({ ...member, isAdmin: true });
      return;
    }
  }
  
  throw new SenderError('User is not a member of this room');
});

// ============================================================================
// MESSAGE REDUCERS
// ============================================================================

spacetimedb.reducer('send_message', { roomId: t.u64(), content: t.string(), parentMessageId: t.u64().optional(), isEphemeral: t.bool(), ephemeralDurationSecs: t.u64().optional() }, (ctx, { roomId, content, parentMessageId, isEphemeral, ephemeralDurationSecs }) => {
  const trimmed = content.trim();
  if (trimmed.length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (trimmed.length > 2000) {
    throw new SenderError('Message too long (max 2000 characters)');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check membership
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString() && !member.isBanned) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Validate parent message if threading
  if (parentMessageId !== undefined) {
    const parentMsg = ctx.db.message.id.find(parentMessageId);
    if (!parentMsg) {
      throw new SenderError('Parent message not found');
    }
    if (parentMsg.roomId !== roomId) {
      throw new SenderError('Parent message is in a different room');
    }
  }
  
  let expiresAt: Timestamp | undefined = undefined;
  if (isEphemeral && ephemeralDurationSecs) {
    const expiryMicros = ctx.timestamp.microsSinceUnixEpoch + (ephemeralDurationSecs * 1000000n);
    expiresAt = new Timestamp(expiryMicros);
  }
  
  const message = ctx.db.message.insert({
    id: 0n,
    roomId,
    senderIdentity: ctx.sender,
    content: trimmed,
    createdAt: ctx.timestamp,
    isEdited: false,
    parentMessageId,
    isEphemeral,
    expiresAt,
  });
  
  // Schedule cleanup for ephemeral messages
  if (isEphemeral && ephemeralDurationSecs) {
    ctx.db.ephemeralMessageCleanup.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(ephemeralDurationSecs * 1000000n),
      messageId: message.id,
    });
  }
  
  // Clear typing indicator
  for (const indicator of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (indicator.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.scheduledId.delete(indicator.scheduledId);
    }
  }
  
  updateActivity(ctx);
});

spacetimedb.reducer('edit_message', { messageId: t.u64(), newContent: t.string() }, (ctx, { messageId, newContent }) => {
  const trimmed = newContent.trim();
  if (trimmed.length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (trimmed.length > 2000) {
    throw new SenderError('Message too long');
  }
  
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  if (message.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('You can only edit your own messages');
  }
  
  // Save edit history
  ctx.db.messageEdit.insert({
    id: 0n,
    messageId,
    previousContent: message.content,
    editedAt: ctx.timestamp,
  });
  
  // Update message
  ctx.db.message.id.update({
    ...message,
    content: trimmed,
    isEdited: true,
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('delete_message', { messageId: t.u64() }, (ctx, { messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  if (message.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
    // Check if user is admin
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
      if (member.userIdentity.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
        isAdmin = true;
        break;
      }
    }
    
    if (!isAdmin) {
      throw new SenderError('You can only delete your own messages or be an admin');
    }
  }
  
  // Delete reactions
  for (const reaction of [...ctx.db.reaction.by_message.filter(messageId)]) {
    ctx.db.reaction.id.delete(reaction.id);
  }
  
  // Delete read receipts
  for (const receipt of [...ctx.db.readReceipt.by_message.filter(messageId)]) {
    ctx.db.readReceipt.id.delete(receipt.id);
  }
  
  // Delete edit history
  for (const edit of [...ctx.db.messageEdit.by_message.filter(messageId)]) {
    ctx.db.messageEdit.id.delete(edit.id);
  }
  
  ctx.db.message.id.delete(messageId);
  updateActivity(ctx);
});

// Scheduled reducer for deleting ephemeral messages
spacetimedb.reducer('delete_ephemeral_message', { arg: EphemeralMessageCleanup.rowType }, (ctx, { arg }) => {
  const message = ctx.db.message.id.find(arg.messageId);
  if (message) {
    // Delete reactions
    for (const reaction of [...ctx.db.reaction.by_message.filter(arg.messageId)]) {
      ctx.db.reaction.id.delete(reaction.id);
    }
    
    // Delete read receipts
    for (const receipt of [...ctx.db.readReceipt.by_message.filter(arg.messageId)]) {
      ctx.db.readReceipt.id.delete(receipt.id);
    }
    
    // Delete edit history
    for (const edit of [...ctx.db.messageEdit.by_message.filter(arg.messageId)]) {
      ctx.db.messageEdit.id.delete(edit.id);
    }
    
    ctx.db.message.id.delete(arg.messageId);
  }
});

// ============================================================================
// SCHEDULED MESSAGE REDUCERS
// ============================================================================

spacetimedb.reducer('schedule_message', { roomId: t.u64(), content: t.string(), scheduledTimeMs: t.u64() }, (ctx, { roomId, content, scheduledTimeMs }) => {
  const trimmed = content.trim();
  if (trimmed.length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (trimmed.length > 2000) {
    throw new SenderError('Message too long');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check membership
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString() && !member.isBanned) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Calculate delay from now
  const nowMs = ctx.timestamp.microsSinceUnixEpoch / 1000n;
  if (scheduledTimeMs <= nowMs) {
    throw new SenderError('Scheduled time must be in the future');
  }
  
  const delayMicros = (scheduledTimeMs - nowMs) * 1000n;
  
  ctx.db.scheduledMessage.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(delayMicros),
    roomId,
    ownerIdentity: ctx.sender,
    content: trimmed,
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('cancel_scheduled_message', { scheduledId: t.u64() }, (ctx, { scheduledId }) => {
  const scheduled = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
  if (!scheduled) {
    throw new SenderError('Scheduled message not found');
  }
  
  if (scheduled.ownerIdentity.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('You can only cancel your own scheduled messages');
  }
  
  ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  updateActivity(ctx);
});

// Scheduled reducer for sending scheduled messages
spacetimedb.reducer('send_scheduled_message', { arg: ScheduledMessage.rowType }, (ctx, { arg }) => {
  const room = ctx.db.room.id.find(arg.roomId);
  if (!room) return;
  
  // Check if user is still a member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(arg.roomId)) {
    if (member.userIdentity.toHexString() === arg.ownerIdentity.toHexString() && !member.isBanned) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) return;
  
  ctx.db.message.insert({
    id: 0n,
    roomId: arg.roomId,
    senderIdentity: arg.ownerIdentity,
    content: arg.content,
    createdAt: ctx.timestamp,
    isEdited: false,
    parentMessageId: undefined,
    isEphemeral: false,
    expiresAt: undefined,
  });
});

// ============================================================================
// TYPING INDICATOR REDUCERS
// ============================================================================

spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check membership
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString() && !member.isBanned) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Remove existing typing indicator for this user in this room
  for (const indicator of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (indicator.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.scheduledId.delete(indicator.scheduledId);
    }
  }
  
  // Add new typing indicator that expires
  ctx.db.typingIndicator.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(TYPING_EXPIRY_MS * 1000n), // Convert to micros
    roomId,
    userIdentity: ctx.sender,
  });
  
  updateActivity(ctx);
});

spacetimedb.reducer('stop_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  for (const indicator of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (indicator.userIdentity.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.scheduledId.delete(indicator.scheduledId);
    }
  }
});

// Scheduled reducer for expiring typing indicators
spacetimedb.reducer('expire_typing', { arg: TypingIndicator.rowType }, (_ctx, { arg: _arg }) => {
  // Row is auto-deleted after reducer completes
});

// ============================================================================
// REACTION REDUCERS
// ============================================================================

spacetimedb.reducer('toggle_reaction', { messageId: t.u64(), emoji: t.string() }, (ctx, { messageId, emoji }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  // Check membership
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
    if (member.userIdentity.toHexString() === ctx.sender.toHexString() && !member.isBanned) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Check if user already has this reaction
  for (const reaction of ctx.db.reaction.by_message.filter(messageId)) {
    if (reaction.userIdentity.toHexString() === ctx.sender.toHexString() && reaction.emoji === emoji) {
      // Remove reaction
      ctx.db.reaction.id.delete(reaction.id);
      return;
    }
  }
  
  // Add reaction
  ctx.db.reaction.insert({
    id: 0n,
    messageId,
    userIdentity: ctx.sender,
    emoji,
  });
  
  updateActivity(ctx);
});

// ============================================================================
// READ RECEIPT REDUCERS
// ============================================================================

spacetimedb.reducer('mark_message_read', { messageId: t.u64() }, (ctx, { messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  // Check membership
  let member = null;
  for (const m of ctx.db.roomMember.by_room.filter(message.roomId)) {
    if (m.userIdentity.toHexString() === ctx.sender.toHexString() && !m.isBanned) {
      member = m;
      break;
    }
  }
  
  if (!member) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Check if already marked read
  for (const receipt of ctx.db.readReceipt.by_message.filter(messageId)) {
    if (receipt.userIdentity.toHexString() === ctx.sender.toHexString()) {
      return; // Already read
    }
  }
  
  ctx.db.readReceipt.insert({
    id: 0n,
    messageId,
    userIdentity: ctx.sender,
    readAt: ctx.timestamp,
  });
  
  // Update last read message
  if (!member.lastReadMessageId || member.lastReadMessageId < messageId) {
    ctx.db.roomMember.id.update({ ...member, lastReadMessageId: messageId });
  }
});

spacetimedb.reducer('mark_room_read', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check membership
  let member = null;
  for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
    if (m.userIdentity.toHexString() === ctx.sender.toHexString() && !m.isBanned) {
      member = m;
      break;
    }
  }
  
  if (!member) {
    throw new SenderError('You are not a member of this room');
  }
  
  // Find latest message ID
  let latestMessageId: bigint | undefined = undefined;
  for (const msg of ctx.db.message.by_room.filter(roomId)) {
    if (latestMessageId === undefined || msg.id > latestMessageId) {
      latestMessageId = msg.id;
    }
  }
  
  if (latestMessageId !== undefined) {
    ctx.db.roomMember.id.update({ ...member, lastReadMessageId: latestMessageId });
  }
  
  // Mark all messages as read
  for (const msg of ctx.db.message.by_room.filter(roomId)) {
    let alreadyRead = false;
    for (const receipt of ctx.db.readReceipt.by_message.filter(msg.id)) {
      if (receipt.userIdentity.toHexString() === ctx.sender.toHexString()) {
        alreadyRead = true;
        break;
      }
    }
    
    if (!alreadyRead) {
      ctx.db.readReceipt.insert({
        id: 0n,
        messageId: msg.id,
        userIdentity: ctx.sender,
        readAt: ctx.timestamp,
      });
    }
  }
});
