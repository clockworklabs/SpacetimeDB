import { SenderError } from 'spacetimedb/server';
import { Timestamp, ScheduleAt } from 'spacetimedb';
import {
  spacetimedb,
  User,
  Room,
  RoomMember,
  BannedUser,
  RoomInvitation,
  Message,
  MessageEdit,
  MessageReaction,
  ReadReceipt,
  TypingIndicator,
  TypingCleanupJob,
  ScheduledMessage,
  ScheduledMessageView,
  EphemeralCleanupJob,
  PresenceAwayJob,
} from './schema';

// ============================================================================
// CONSTANTS
// ============================================================================

const TYPING_TIMEOUT_SECONDS = 5n;
const PRESENCE_AWAY_TIMEOUT_SECONDS = 300n; // 5 minutes

// ============================================================================
// LIFECYCLE REDUCERS
// ============================================================================

spacetimedb.clientConnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: true,
      status: user.status === 'invisible' ? 'invisible' : 'online',
      lastActiveAt: ctx.timestamp,
      connectionId: ctx.connectionId,
    });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: undefined,
      online: true,
      status: 'online',
      lastActiveAt: ctx.timestamp,
      connectionId: ctx.connectionId,
    });
  }
  
  // Schedule auto-away
  schedulePresenceAway(ctx, ctx.sender);
});

spacetimedb.clientDisconnected((ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: false,
      lastActiveAt: ctx.timestamp,
      connectionId: undefined,
    });
  }
  
  // Clean up typing indicators
  for (const typing of ctx.db.typingIndicator.by_user.filter(ctx.sender)) {
    ctx.db.typingIndicator.id.delete(typing.id);
  }
});

// ============================================================================
// USER REDUCERS
// ============================================================================

// Set user display name
spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  if (!name || name.trim().length === 0) {
    throw new SenderError('Name cannot be empty');
  }
  if (name.length > 50) {
    throw new SenderError('Name too long (max 50 characters)');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('User not found');
  }
  
  ctx.db.user.identity.update({ ...user, name: name.trim() });
});

// Set user status (online, away, dnd, invisible)
spacetimedb.reducer('set_status', { status: t.string() }, (ctx, { status }) => {
  const validStatuses = ['online', 'away', 'dnd', 'invisible'];
  if (!validStatuses.includes(status)) {
    throw new SenderError('Invalid status');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('User not found');
  }
  
  ctx.db.user.identity.update({
    ...user,
    status,
    lastActiveAt: ctx.timestamp,
  });
});

// Update activity (heartbeat for presence)
spacetimedb.reducer('update_activity', {}, (ctx) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('User not found');
  }
  
  // Only update if not invisible
  if (user.status !== 'invisible') {
    ctx.db.user.identity.update({
      ...user,
      status: 'online',
      lastActiveAt: ctx.timestamp,
    });
  }
  
  // Reschedule auto-away
  schedulePresenceAway(ctx, ctx.sender);
});

// ============================================================================
// ROOM REDUCERS
// ============================================================================

// Create a new room
spacetimedb.reducer('create_room', { name: t.string(), isPrivate: t.bool() }, (ctx, { name, isPrivate }) => {
  if (!name || name.trim().length === 0) {
    throw new SenderError('Room name cannot be empty');
  }
  if (name.length > 100) {
    throw new SenderError('Room name too long (max 100 characters)');
  }
  
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) {
    throw new SenderError('Set your name first');
  }
  
  // Create room
  const room = ctx.db.room.insert({
    id: 0n,
    name: name.trim(),
    ownerId: ctx.sender,
    isPrivate,
    isDm: false,
    createdAt: ctx.timestamp,
  });
  
  // Add creator as member and admin
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: room.id,
    userId: ctx.sender,
    isAdmin: true,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
    lastReadAt: undefined,
  });
});

// Join a public room
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
    throw new SenderError('Set your name first');
  }
  
  // Check if banned
  for (const ban of ctx.db.bannedUser.by_room.filter(roomId)) {
    if (ban.userId.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('You are banned from this room');
    }
  }
  
  // Check if already a member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Already a member');
    }
  }
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    isAdmin: false,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
    lastReadAt: undefined,
  });
});

// Leave a room
spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  let membershipId: bigint | undefined;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      membershipId = member.id;
      break;
    }
  }
  
  if (membershipId === undefined) {
    throw new SenderError('Not a member');
  }
  
  ctx.db.roomMember.id.delete(membershipId);
});

// ============================================================================
// PRIVATE ROOM / DM REDUCERS
// ============================================================================

// Start a DM with another user
spacetimedb.reducer('start_dm', { targetUserName: t.string() }, (ctx, { targetUserName }) => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user || !user.name) {
    throw new SenderError('Set your name first');
  }
  
  // Find target user
  let targetUser: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(targetUserName)) {
    targetUser = u;
    break;
  }
  
  if (!targetUser) {
    throw new SenderError('User not found');
  }
  
  if (targetUser.identity.toHexString() === ctx.sender.toHexString()) {
    throw new SenderError('Cannot DM yourself');
  }
  
  // Check if DM already exists
  for (const room of ctx.db.room.iter()) {
    if (room.isDm) {
      const members = [...ctx.db.roomMember.by_room.filter(room.id)];
      if (members.length === 2) {
        const memberIds = members.map(m => m.userId.toHexString());
        if (memberIds.includes(ctx.sender.toHexString()) && 
            memberIds.includes(targetUser.identity.toHexString())) {
          throw new SenderError('DM already exists');
        }
      }
    }
  }
  
  // Create DM room
  const dmRoom = ctx.db.room.insert({
    id: 0n,
    name: `DM: ${user.name} & ${targetUser.name}`,
    ownerId: ctx.sender,
    isPrivate: true,
    isDm: true,
    createdAt: ctx.timestamp,
  });
  
  // Add both users as members
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: dmRoom.id,
    userId: ctx.sender,
    isAdmin: true,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
    lastReadAt: undefined,
  });
  
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: dmRoom.id,
    userId: targetUser.identity,
    isAdmin: true,
    joinedAt: ctx.timestamp,
    lastReadMessageId: undefined,
    lastReadAt: undefined,
  });
});

// Invite user to private room
spacetimedb.reducer('invite_to_room', { roomId: t.u64(), inviteeName: t.string() }, (ctx, { roomId, inviteeName }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  if (!room.isPrivate) {
    throw new SenderError('Room is not private');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
      isAdmin = true;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can invite');
  }
  
  // Find invitee
  let invitee: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(inviteeName)) {
    invitee = u;
    break;
  }
  
  if (!invitee) {
    throw new SenderError('User not found');
  }
  
  // Check if already a member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === invitee.identity.toHexString()) {
      throw new SenderError('User is already a member');
    }
  }
  
  // Check if already invited
  for (const inv of ctx.db.roomInvitation.by_invitee.filter(invitee.identity)) {
    if (inv.roomId === roomId && inv.status === 'pending') {
      throw new SenderError('User already has a pending invitation');
    }
  }
  
  ctx.db.roomInvitation.insert({
    id: 0n,
    roomId,
    inviterId: ctx.sender,
    inviteeId: invitee.identity,
    status: 'pending',
    createdAt: ctx.timestamp,
  });
});

// Respond to invitation
spacetimedb.reducer('respond_to_invitation', { invitationId: t.u64(), accept: t.bool() }, (ctx, { invitationId, accept }) => {
  const invitation = ctx.db.roomInvitation.id.find(invitationId);
  if (!invitation) {
    throw new SenderError('Invitation not found');
  }
  
  if (invitation.inviteeId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Not your invitation');
  }
  
  if (invitation.status !== 'pending') {
    throw new SenderError('Invitation already responded');
  }
  
  const room = ctx.db.room.id.find(invitation.roomId);
  if (!room) {
    throw new SenderError('Room no longer exists');
  }
  
  if (accept) {
    ctx.db.roomInvitation.id.update({ ...invitation, status: 'accepted' });
    
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: invitation.roomId,
      userId: ctx.sender,
      isAdmin: false,
      joinedAt: ctx.timestamp,
      lastReadMessageId: undefined,
      lastReadAt: undefined,
    });
  } else {
    ctx.db.roomInvitation.id.update({ ...invitation, status: 'declined' });
  }
});

// ============================================================================
// PERMISSION REDUCERS
// ============================================================================

// Kick user from room
spacetimedb.reducer('kick_user', { roomId: t.u64(), targetUserName: t.string() }, (ctx, { roomId, targetUserName }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
      isAdmin = true;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can kick users');
  }
  
  // Find target user
  let targetUser: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(targetUserName)) {
    targetUser = u;
    break;
  }
  
  if (!targetUser) {
    throw new SenderError('User not found');
  }
  
  if (targetUser.identity.toHexString() === room.ownerId.toHexString()) {
    throw new SenderError('Cannot kick room owner');
  }
  
  // Find and remove membership
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === targetUser.identity.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  
  throw new SenderError('User is not a member');
});

// Ban user from room
spacetimedb.reducer('ban_user', { roomId: t.u64(), targetUserName: t.string(), reason: t.string().optional() }, (ctx, { roomId, targetUserName, reason }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
      isAdmin = true;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can ban users');
  }
  
  // Find target user
  let targetUser: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(targetUserName)) {
    targetUser = u;
    break;
  }
  
  if (!targetUser) {
    throw new SenderError('User not found');
  }
  
  if (targetUser.identity.toHexString() === room.ownerId.toHexString()) {
    throw new SenderError('Cannot ban room owner');
  }
  
  // Remove from room if member
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === targetUser.identity.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      break;
    }
  }
  
  // Add ban
  ctx.db.bannedUser.insert({
    id: 0n,
    roomId,
    userId: targetUser.identity,
    bannedBy: ctx.sender,
    bannedAt: ctx.timestamp,
    reason: reason || undefined,
  });
});

// Unban user
spacetimedb.reducer('unban_user', { roomId: t.u64(), targetUserName: t.string() }, (ctx, { roomId, targetUserName }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
      isAdmin = true;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can unban users');
  }
  
  // Find target user
  let targetUser: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(targetUserName)) {
    targetUser = u;
    break;
  }
  
  if (!targetUser) {
    throw new SenderError('User not found');
  }
  
  // Remove ban
  for (const ban of ctx.db.bannedUser.by_room.filter(roomId)) {
    if (ban.userId.toHexString() === targetUser.identity.toHexString()) {
      ctx.db.bannedUser.id.delete(ban.id);
      return;
    }
  }
  
  throw new SenderError('User is not banned');
});

// Promote user to admin
spacetimedb.reducer('promote_to_admin', { roomId: t.u64(), targetUserName: t.string() }, (ctx, { roomId, targetUserName }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if sender is admin
  let isAdmin = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
      isAdmin = true;
      break;
    }
  }
  
  if (!isAdmin) {
    throw new SenderError('Only admins can promote users');
  }
  
  // Find target user
  let targetUser: typeof User.rowType | undefined;
  for (const u of ctx.db.user.by_name.filter(targetUserName)) {
    targetUser = u;
    break;
  }
  
  if (!targetUser) {
    throw new SenderError('User not found');
  }
  
  // Promote
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === targetUser.identity.toHexString()) {
      if (member.isAdmin) {
        throw new SenderError('User is already an admin');
      }
      ctx.db.roomMember.id.update({ ...member, isAdmin: true });
      return;
    }
  }
  
  throw new SenderError('User is not a member');
});

// ============================================================================
// MESSAGE REDUCERS
// ============================================================================

// Send a message
spacetimedb.reducer('send_message', { roomId: t.u64(), content: t.string(), parentMessageId: t.u64().optional() }, (ctx, { roomId, content, parentMessageId }) => {
  if (!content || content.trim().length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (content.length > 4000) {
    throw new SenderError('Message too long (max 4000 characters)');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('Not a member of this room');
  }
  
  // If replying, validate parent message
  if (parentMessageId !== undefined) {
    const parent = ctx.db.message.id.find(parentMessageId);
    if (!parent) {
      throw new SenderError('Parent message not found');
    }
    if (parent.roomId !== roomId) {
      throw new SenderError('Parent message is in different room');
    }
    // Increment reply count on parent
    ctx.db.message.id.update({ ...parent, replyCount: parent.replyCount + 1 });
  }
  
  ctx.db.message.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
    editedAt: undefined,
    isEdited: false,
    parentMessageId,
    replyCount: 0,
    expiresAt: undefined,
  });
  
  // Clear typing indicator
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
      break;
    }
  }
});

// Send ephemeral message
spacetimedb.reducer('send_ephemeral_message', { roomId: t.u64(), content: t.string(), durationSeconds: t.u64() }, (ctx, { roomId, content, durationSeconds }) => {
  if (!content || content.trim().length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (content.length > 4000) {
    throw new SenderError('Message too long');
  }
  if (durationSeconds < 10n || durationSeconds > 3600n) {
    throw new SenderError('Duration must be between 10 seconds and 1 hour');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('Not a member of this room');
  }
  
  const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + (durationSeconds * 1_000_000n);
  const expiresAt = new Timestamp(expiresAtMicros);
  
  const message = ctx.db.message.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
    editedAt: undefined,
    isEdited: false,
    parentMessageId: undefined,
    replyCount: 0,
    expiresAt,
  });
  
  // Schedule cleanup
  ctx.db.ephemeralCleanupJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(expiresAtMicros),
    messageId: message.id,
  });
});

// Edit message
spacetimedb.reducer('edit_message', { messageId: t.u64(), newContent: t.string() }, (ctx, { messageId, newContent }) => {
  if (!newContent || newContent.trim().length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (newContent.length > 4000) {
    throw new SenderError('Message too long');
  }
  
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  if (message.senderId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Can only edit your own messages');
  }
  
  // Store edit history
  ctx.db.messageEdit.insert({
    id: 0n,
    messageId,
    previousContent: message.content,
    editedAt: ctx.timestamp,
  });
  
  ctx.db.message.id.update({
    ...message,
    content: newContent.trim(),
    isEdited: true,
    editedAt: ctx.timestamp,
  });
});

// Delete message
spacetimedb.reducer('delete_message', { messageId: t.u64() }, (ctx, { messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  // Check if sender or admin
  let canDelete = message.senderId.toHexString() === ctx.sender.toHexString();
  
  if (!canDelete) {
    for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString() && member.isAdmin) {
        canDelete = true;
        break;
      }
    }
  }
  
  if (!canDelete) {
    throw new SenderError('Cannot delete this message');
  }
  
  // Delete reactions
  for (const reaction of ctx.db.messageReaction.by_message.filter(messageId)) {
    ctx.db.messageReaction.id.delete(reaction.id);
  }
  
  // Delete read receipts
  for (const receipt of ctx.db.readReceipt.by_message.filter(messageId)) {
    ctx.db.readReceipt.id.delete(receipt.id);
  }
  
  // Delete edit history
  for (const edit of ctx.db.messageEdit.by_message.filter(messageId)) {
    ctx.db.messageEdit.id.delete(edit.id);
  }
  
  // If parent, update reply counts
  if (message.parentMessageId !== undefined) {
    const parent = ctx.db.message.id.find(message.parentMessageId);
    if (parent && parent.replyCount > 0) {
      ctx.db.message.id.update({ ...parent, replyCount: parent.replyCount - 1 });
    }
  }
  
  ctx.db.message.id.delete(messageId);
});

// ============================================================================
// REACTION REDUCERS
// ============================================================================

// Toggle reaction
spacetimedb.reducer('toggle_reaction', { messageId: t.u64(), emoji: t.string() }, (ctx, { messageId, emoji }) => {
  const validEmojis = ['👍', '❤️', '😂', '😮', '😢', '👎', '🎉', '🔥'];
  if (!validEmojis.includes(emoji)) {
    throw new SenderError('Invalid emoji');
  }
  
  const message = ctx.db.message.id.find(messageId);
  if (!message) {
    throw new SenderError('Message not found');
  }
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('Not a member of this room');
  }
  
  // Check if already reacted with this emoji
  for (const reaction of ctx.db.messageReaction.by_message.filter(messageId)) {
    if (reaction.userId.toHexString() === ctx.sender.toHexString() && reaction.emoji === emoji) {
      // Remove reaction
      ctx.db.messageReaction.id.delete(reaction.id);
      return;
    }
  }
  
  // Add reaction
  ctx.db.messageReaction.insert({
    id: 0n,
    messageId,
    userId: ctx.sender,
    emoji,
    createdAt: ctx.timestamp,
  });
});

// ============================================================================
// TYPING INDICATOR REDUCERS
// ============================================================================

// Start typing
spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('Not a member of this room');
  }
  
  const expiresAtMicros = ctx.timestamp.microsSinceUnixEpoch + (TYPING_TIMEOUT_SECONDS * 1_000_000n);
  const expiresAt = new Timestamp(expiresAtMicros);
  
  // Update existing or create new
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.update({
        ...typing,
        startedAt: ctx.timestamp,
        expiresAt,
      });
      return;
    }
  }
  
  const indicator = ctx.db.typingIndicator.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    startedAt: ctx.timestamp,
    expiresAt,
  });
  
  // Schedule cleanup
  ctx.db.typingCleanupJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(expiresAtMicros),
    typingIndicatorId: indicator.id,
  });
});

// Stop typing
spacetimedb.reducer('stop_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
      return;
    }
  }
});

// ============================================================================
// READ RECEIPT REDUCERS
// ============================================================================

// Mark messages as read
spacetimedb.reducer('mark_messages_read', { roomId: t.u64(), upToMessageId: t.u64() }, (ctx, { roomId, upToMessageId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  const message = ctx.db.message.id.find(upToMessageId);
  if (!message || message.roomId !== roomId) {
    throw new SenderError('Message not found');
  }
  
  // Update room member's last read
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.roomMember.id.update({
        ...member,
        lastReadMessageId: upToMessageId,
        lastReadAt: ctx.timestamp,
      });
      break;
    }
  }
  
  // Add read receipt for this specific message
  // Check if already exists
  for (const receipt of ctx.db.readReceipt.by_message.filter(upToMessageId)) {
    if (receipt.userId.toHexString() === ctx.sender.toHexString()) {
      return; // Already marked
    }
  }
  
  ctx.db.readReceipt.insert({
    id: 0n,
    messageId: upToMessageId,
    userId: ctx.sender,
    seenAt: ctx.timestamp,
  });
});

// ============================================================================
// SCHEDULED MESSAGE REDUCERS
// ============================================================================

// Schedule a message
spacetimedb.reducer('schedule_message', { roomId: t.u64(), content: t.string(), sendAtMicros: t.u64() }, (ctx, { roomId, content, sendAtMicros }) => {
  if (!content || content.trim().length === 0) {
    throw new SenderError('Message cannot be empty');
  }
  if (content.length > 4000) {
    throw new SenderError('Message too long');
  }
  
  const room = ctx.db.room.id.find(roomId);
  if (!room) {
    throw new SenderError('Room not found');
  }
  
  // Check if member
  let isMember = false;
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      isMember = true;
      break;
    }
  }
  
  if (!isMember) {
    throw new SenderError('Not a member of this room');
  }
  
  const sendAtBigInt = BigInt(sendAtMicros);
  if (sendAtBigInt <= ctx.timestamp.microsSinceUnixEpoch) {
    throw new SenderError('Scheduled time must be in the future');
  }
  
  // Insert scheduled message
  ctx.db.scheduledMessage.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(sendAtBigInt),
    roomId,
    senderId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
  });
  
  // Add to view for user visibility
  ctx.db.scheduledMessageView.insert({
    id: 0n,
    roomId,
    senderId: ctx.sender,
    content: content.trim(),
    scheduledFor: new Timestamp(sendAtBigInt),
    createdAt: ctx.timestamp,
  });
});

// Cancel scheduled message
spacetimedb.reducer('cancel_scheduled_message', { viewId: t.u64() }, (ctx, { viewId }) => {
  const view = ctx.db.scheduledMessageView.id.find(viewId);
  if (!view) {
    throw new SenderError('Scheduled message not found');
  }
  
  if (view.senderId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('Can only cancel your own scheduled messages');
  }
  
  // Find and delete the actual scheduled job
  for (const job of ctx.db.scheduledMessage.iter()) {
    if (job.senderId.toHexString() === ctx.sender.toHexString() &&
        job.roomId === view.roomId &&
        job.content === view.content) {
      ctx.db.scheduledMessage.scheduledId.delete(job.scheduledId);
      break;
    }
  }
  
  ctx.db.scheduledMessageView.id.delete(viewId);
});

// ============================================================================
// SCHEDULED REDUCER HANDLERS
// ============================================================================

// Run scheduled message
spacetimedb.reducer('run_scheduled_message', { arg: ScheduledMessage.rowType }, (ctx, { arg }) => {
  const room = ctx.db.room.id.find(arg.roomId);
  if (!room) {
    // Room was deleted, remove view entry
    for (const view of ctx.db.scheduledMessageView.by_sender.filter(arg.senderId)) {
      if (view.content === arg.content && view.roomId === arg.roomId) {
        ctx.db.scheduledMessageView.id.delete(view.id);
        break;
      }
    }
    return;
  }
  
  // Check if still a member
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
      isEdited: false,
      parentMessageId: undefined,
      replyCount: 0,
      expiresAt: undefined,
    });
  }
  
  // Remove from view
  for (const view of ctx.db.scheduledMessageView.by_sender.filter(arg.senderId)) {
    if (view.content === arg.content && view.roomId === arg.roomId) {
      ctx.db.scheduledMessageView.id.delete(view.id);
      break;
    }
  }
});

// Run ephemeral cleanup
spacetimedb.reducer('run_ephemeral_cleanup', { arg: EphemeralCleanupJob.rowType }, (ctx, { arg }) => {
  const message = ctx.db.message.id.find(arg.messageId);
  if (!message) {
    return; // Already deleted
  }
  
  // Delete reactions
  for (const reaction of ctx.db.messageReaction.by_message.filter(arg.messageId)) {
    ctx.db.messageReaction.id.delete(reaction.id);
  }
  
  // Delete read receipts
  for (const receipt of ctx.db.readReceipt.by_message.filter(arg.messageId)) {
    ctx.db.readReceipt.id.delete(receipt.id);
  }
  
  ctx.db.message.id.delete(arg.messageId);
});

// Run typing cleanup
spacetimedb.reducer('run_typing_cleanup', { arg: TypingCleanupJob.rowType }, (ctx, { arg }) => {
  const indicator = ctx.db.typingIndicator.id.find(arg.typingIndicatorId);
  if (!indicator) {
    return; // Already removed
  }
  
  // Only delete if expired
  if (indicator.expiresAt.microsSinceUnixEpoch <= ctx.timestamp.microsSinceUnixEpoch) {
    ctx.db.typingIndicator.id.delete(indicator.id);
  }
});

// Run presence away
spacetimedb.reducer('run_presence_away', { arg: PresenceAwayJob.rowType }, (ctx, { arg }) => {
  const user = ctx.db.user.identity.find(arg.userId);
  if (!user) {
    return;
  }
  
  // Only set to away if still online and not invisible
  if (user.online && user.status === 'online') {
    // Check if they've been inactive
    if (user.lastActiveAt) {
      const inactiveFor = ctx.timestamp.microsSinceUnixEpoch - user.lastActiveAt.microsSinceUnixEpoch;
      if (inactiveFor >= PRESENCE_AWAY_TIMEOUT_SECONDS * 1_000_000n) {
        ctx.db.user.identity.update({ ...user, status: 'away' });
      }
    }
  }
});

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

function schedulePresenceAway(ctx: any, userId: typeof User.rowType['identity']) {
  // Cancel existing job
  for (const job of ctx.db.presenceAwayJob.iter()) {
    if (job.userId.toHexString() === userId.toHexString()) {
      ctx.db.presenceAwayJob.scheduledId.delete(job.scheduledId);
      break;
    }
  }
  
  // Schedule new away job
  const awayAtMicros = ctx.timestamp.microsSinceUnixEpoch + (PRESENCE_AWAY_TIMEOUT_SECONDS * 1_000_000n);
  ctx.db.presenceAwayJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(awayAtMicros),
    userId,
  });
}

// Need to import t for reducer args
import { t } from 'spacetimedb/server';

export { spacetimedb };
