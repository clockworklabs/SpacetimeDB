import { SenderError } from 'spacetimedb/server';
import { Timestamp, ScheduleAt } from 'spacetimedb';
import {
  spacetimedb,
  User,
  Room,
  RoomMember,
  RoomBan,
  RoomInvite,
  Message,
  MessageEdit,
  MessageReaction,
  ReadReceipt,
  TypingIndicator,
  TypingExpiry,
  ScheduledMessage,
  EphemeralMessageCleanup,
  AutoAwayCheck,
  UserStatus,
} from './schema';
import { t } from 'spacetimedb/server';

// ============================================================================
// CONNECTION LIFECYCLE
// ============================================================================

spacetimedb.clientConnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: true,
      status: UserStatus.ONLINE,
      lastActive: ctx.timestamp,
    });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: undefined,
      status: UserStatus.ONLINE,
      lastActive: ctx.timestamp,
      online: true,
    });
  }

  // Schedule auto-away check in 5 minutes
  const fiveMins = ctx.timestamp.microsSinceUnixEpoch + 300_000_000n;
  ctx.db.autoAwayCheck.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(fiveMins),
    userId: ctx.sender,
  });
});

spacetimedb.clientDisconnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: false,
      status: UserStatus.OFFLINE,
      lastActive: ctx.timestamp,
    });
  }

  // Remove typing indicators for this user
  for (const typing of ctx.db.typingIndicator.iter()) {
    if (typing.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(typing.id);
    }
  }
});

// ============================================================================
// USER REDUCERS
// ============================================================================

spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  const trimmed = name.trim();
  if (!trimmed || trimmed.length < 1 || trimmed.length > 50) {
    throw new SenderError('Name must be 1-50 characters');
  }

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  ctx.db.user.identity.update({ ...user, name: trimmed });
});

spacetimedb.reducer('set_status', { status: t.string() }, (ctx, { status }) => {
  const validStatuses = [
    UserStatus.ONLINE,
    UserStatus.AWAY,
    UserStatus.DND,
    UserStatus.INVISIBLE,
  ];
  if (!validStatuses.includes(status as (typeof validStatuses)[number])) {
    throw new SenderError('Invalid status');
  }

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  ctx.db.user.identity.update({
    ...user,
    status,
    lastActive: ctx.timestamp,
  });
});

spacetimedb.reducer('heartbeat', {}, ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  ctx.db.user.identity.update({
    ...user,
    lastActive: ctx.timestamp,
  });

  // If user was away, set back to online
  if (user.status === UserStatus.AWAY) {
    ctx.db.user.identity.update({
      ...user,
      status: UserStatus.ONLINE,
      lastActive: ctx.timestamp,
    });
  }
});

// Auto-away scheduler
spacetimedb.reducer(
  'check_auto_away',
  { arg: AutoAwayCheck.rowType },
  (ctx, { arg }) => {
    const user = ctx.db.user.identity.find(arg.userId);
    if (!user || !user.online) return;

    // If user hasn't been active for 5 minutes, set to away
    const fiveMinsAgo = ctx.timestamp.microsSinceUnixEpoch - 300_000_000n;
    if (
      user.lastActive.microsSinceUnixEpoch < fiveMinsAgo &&
      user.status === UserStatus.ONLINE
    ) {
      ctx.db.user.identity.update({
        ...user,
        status: UserStatus.AWAY,
      });
    }

    // Schedule next check in 5 minutes
    const fiveMins = ctx.timestamp.microsSinceUnixEpoch + 300_000_000n;
    ctx.db.autoAwayCheck.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(fiveMins),
      userId: arg.userId,
    });
  }
);

// ============================================================================
// ROOM REDUCERS
// ============================================================================

spacetimedb.reducer(
  'create_room',
  { name: t.string(), isPrivate: t.bool() },
  (ctx, { name, isPrivate }) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed.length < 1 || trimmed.length > 100) {
      throw new SenderError('Room name must be 1-100 characters');
    }

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');

    const roomId = ctx.db.room.insert({
      id: 0n,
      name: trimmed,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
      isPrivate,
      isDm: false,
    }).id;

    // Creator automatically joins and is admin
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userId: ctx.sender,
      isAdmin: true,
      joinedAt: ctx.timestamp,
    });
  }
);

spacetimedb.reducer('join_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');

  // Check if private room
  if (room.isPrivate) {
    // Must have a pending invitation
    let hasInvite = false;
    for (const invite of ctx.db.roomInvite.by_room.filter(roomId)) {
      if (
        invite.inviteeId.toHexString() === ctx.sender.toHexString() &&
        invite.status === 'pending'
      ) {
        hasInvite = true;
        break;
      }
    }
    if (!hasInvite) {
      throw new SenderError('Private room - invitation required');
    }
  }

  // Check if banned
  for (const ban of ctx.db.roomBan.by_room.filter(roomId)) {
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
  });
});

spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
    if (member.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.roomMember.id.delete(member.id);
      return;
    }
  }
  throw new SenderError('Not a member of this room');
});

spacetimedb.reducer(
  'invite_to_room',
  { roomId: t.u64(), inviteeName: t.string() },
  (ctx, { roomId, inviteeName }) => {
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');

    // Check if sender is admin
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (
        member.userId.toHexString() === ctx.sender.toHexString() &&
        member.isAdmin
      ) {
        isAdmin = true;
        break;
      }
    }
    if (!isAdmin) throw new SenderError('Only admins can invite');

    // Find invitee by name
    let invitee = null;
    for (const user of ctx.db.user.iter()) {
      if (user.name === inviteeName) {
        invitee = user;
        break;
      }
    }
    if (!invitee) throw new SenderError('User not found');

    // Check if already invited
    for (const invite of ctx.db.roomInvite.by_room.filter(roomId)) {
      if (
        invite.inviteeId.toHexString() === invitee.identity.toHexString() &&
        invite.status === 'pending'
      ) {
        throw new SenderError('User already has a pending invitation');
      }
    }

    ctx.db.roomInvite.insert({
      id: 0n,
      roomId,
      inviterId: ctx.sender,
      inviteeId: invitee.identity,
      createdAt: ctx.timestamp,
      status: 'pending',
    });
  }
);

spacetimedb.reducer(
  'respond_to_invite',
  { inviteId: t.u64(), accept: t.bool() },
  (ctx, { inviteId, accept }) => {
    const invite = ctx.db.roomInvite.id.find(inviteId);
    if (!invite) throw new SenderError('Invitation not found');

    if (invite.inviteeId.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Not your invitation');
    }

    if (invite.status !== 'pending') {
      throw new SenderError('Invitation already responded to');
    }

    ctx.db.roomInvite.id.update({
      ...invite,
      status: accept ? 'accepted' : 'declined',
    });

    if (accept) {
      const room = ctx.db.room.id.find(invite.roomId);
      if (!room) throw new SenderError('Room no longer exists');

      ctx.db.roomMember.insert({
        id: 0n,
        roomId: invite.roomId,
        userId: ctx.sender,
        isAdmin: false,
        joinedAt: ctx.timestamp,
      });
    }
  }
);

spacetimedb.reducer(
  'kick_user',
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    // Check if sender is admin
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (
        member.userId.toHexString() === ctx.sender.toHexString() &&
        member.isAdmin
      ) {
        isAdmin = true;
        break;
      }
    }
    if (!isAdmin) throw new SenderError('Only admins can kick users');

    // Can't kick yourself
    if (userId.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Cannot kick yourself');
    }

    // Remove member
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === userId.toHexString()) {
        ctx.db.roomMember.id.delete(member.id);
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

spacetimedb.reducer(
  'ban_user',
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    // Check if sender is admin
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (
        member.userId.toHexString() === ctx.sender.toHexString() &&
        member.isAdmin
      ) {
        isAdmin = true;
        break;
      }
    }
    if (!isAdmin) throw new SenderError('Only admins can ban users');

    // Can't ban yourself
    if (userId.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Cannot ban yourself');
    }

    // Check if already banned
    for (const ban of ctx.db.roomBan.by_room.filter(roomId)) {
      if (ban.userId.toHexString() === userId.toHexString()) {
        throw new SenderError('User already banned');
      }
    }

    // Remove member if present
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === userId.toHexString()) {
        ctx.db.roomMember.id.delete(member.id);
        break;
      }
    }

    ctx.db.roomBan.insert({
      id: 0n,
      roomId,
      userId,
      bannedBy: ctx.sender,
      bannedAt: ctx.timestamp,
    });
  }
);

spacetimedb.reducer(
  'promote_to_admin',
  { roomId: t.u64(), userId: t.identity() },
  (ctx, { roomId, userId }) => {
    // Check if sender is admin
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (
        member.userId.toHexString() === ctx.sender.toHexString() &&
        member.isAdmin
      ) {
        isAdmin = true;
        break;
      }
    }
    if (!isAdmin) throw new SenderError('Only admins can promote users');

    // Find and update target member
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === userId.toHexString()) {
        ctx.db.roomMember.id.update({ ...member, isAdmin: true });
        return;
      }
    }
    throw new SenderError('User not in room');
  }
);

// ============================================================================
// DIRECT MESSAGES
// ============================================================================

spacetimedb.reducer(
  'start_dm',
  { targetName: t.string() },
  (ctx, { targetName }) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user || !user.name) throw new SenderError('Set your name first');

    // Find target user
    let target = null;
    for (const u of ctx.db.user.iter()) {
      if (u.name === targetName) {
        target = u;
        break;
      }
    }
    if (!target) throw new SenderError('User not found');

    if (target.identity.toHexString() === ctx.sender.toHexString()) {
      throw new SenderError('Cannot DM yourself');
    }

    // Check if DM room already exists between these users
    for (const room of ctx.db.room.iter()) {
      if (room.isDm) {
        const members = [...ctx.db.roomMember.by_room.filter(room.id)];
        if (members.length === 2) {
          const memberIds = members.map(m => m.userId.toHexString());
          if (
            memberIds.includes(ctx.sender.toHexString()) &&
            memberIds.includes(target.identity.toHexString())
          ) {
            throw new SenderError('DM already exists');
          }
        }
      }
    }

    // Create DM room
    const roomName = `DM: ${user.name} & ${target.name}`;
    const roomId = ctx.db.room.insert({
      id: 0n,
      name: roomName,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
      isPrivate: true,
      isDm: true,
    }).id;

    // Add both users
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userId: ctx.sender,
      isAdmin: true,
      joinedAt: ctx.timestamp,
    });

    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userId: target.identity,
      isAdmin: true,
      joinedAt: ctx.timestamp,
    });
  }
);

// ============================================================================
// MESSAGE REDUCERS
// ============================================================================

spacetimedb.reducer(
  'send_message',
  { roomId: t.u64(), content: t.string() },
  (ctx, { roomId, content }) => {
    const trimmed = content.trim();
    if (!trimmed || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    ctx.db.message.insert({
      id: 0n,
      roomId,
      senderId: ctx.sender,
      content: trimmed,
      createdAt: ctx.timestamp,
      editedAt: undefined,
      isEdited: false,
      threadParentId: undefined,
      expiresAt: undefined,
    });

    // Clear typing indicator for this user
    for (const typing of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (typing.userId.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(typing.id);
      }
    }
  }
);

spacetimedb.reducer(
  'send_ephemeral_message',
  { roomId: t.u64(), content: t.string(), durationSecs: t.u64() },
  (ctx, { roomId, content, durationSecs }) => {
    const trimmed = content.trim();
    if (!trimmed || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    if (durationSecs < 10n || durationSecs > 3600n) {
      throw new SenderError('Duration must be 10-3600 seconds');
    }

    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const expiresAtMicros =
      ctx.timestamp.microsSinceUnixEpoch + durationSecs * 1_000_000n;

    const messageId = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderId: ctx.sender,
      content: trimmed,
      createdAt: ctx.timestamp,
      editedAt: undefined,
      isEdited: false,
      threadParentId: undefined,
      expiresAt: new Timestamp(expiresAtMicros),
    }).id;

    // Schedule cleanup
    ctx.db.ephemeralMessageCleanup.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAtMicros),
      messageId,
    });
  }
);

spacetimedb.reducer(
  'cleanup_ephemeral_message',
  { arg: EphemeralMessageCleanup.rowType },
  (ctx, { arg }) => {
    const message = ctx.db.message.id.find(arg.messageId);
    if (message) {
      // Delete associated reactions
      for (const reaction of ctx.db.messageReaction.by_message.filter(
        arg.messageId
      )) {
        ctx.db.messageReaction.id.delete(reaction.id);
      }
      // Delete associated read receipts
      for (const receipt of ctx.db.readReceipt.by_message.filter(
        arg.messageId
      )) {
        ctx.db.readReceipt.id.delete(receipt.id);
      }
      // Delete edit history
      for (const edit of ctx.db.messageEdit.by_message.filter(arg.messageId)) {
        ctx.db.messageEdit.id.delete(edit.id);
      }
      // Delete the message
      ctx.db.message.id.delete(arg.messageId);
    }
  }
);

spacetimedb.reducer(
  'reply_to_message',
  { messageId: t.u64(), content: t.string() },
  (ctx, { messageId, content }) => {
    const parentMessage = ctx.db.message.id.find(messageId);
    if (!parentMessage) throw new SenderError('Message not found');

    const trimmed = content.trim();
    if (!trimmed || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(
      parentMessage.roomId
    )) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    // If parent is already a reply, use its parent (flatten to one level)
    const threadParentId = parentMessage.threadParentId ?? parentMessage.id;

    ctx.db.message.insert({
      id: 0n,
      roomId: parentMessage.roomId,
      senderId: ctx.sender,
      content: trimmed,
      createdAt: ctx.timestamp,
      editedAt: undefined,
      isEdited: false,
      threadParentId,
      expiresAt: undefined,
    });
  }
);

spacetimedb.reducer(
  'edit_message',
  { messageId: t.u64(), newContent: t.string() },
  (ctx, { messageId, newContent }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    if (message.senderId.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Can only edit your own messages');
    }

    const trimmed = newContent.trim();
    if (!trimmed || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    // Store previous version in edit history
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      previousContent: message.content,
      editedAt: ctx.timestamp,
    });

    ctx.db.message.id.update({
      ...message,
      content: trimmed,
      editedAt: ctx.timestamp,
      isEdited: true,
    });
  }
);

spacetimedb.reducer(
  'delete_message',
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    // Check if sender is the author or a room admin
    const isAuthor =
      message.senderId.toHexString() === ctx.sender.toHexString();
    let isAdmin = false;
    for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
      if (
        member.userId.toHexString() === ctx.sender.toHexString() &&
        member.isAdmin
      ) {
        isAdmin = true;
        break;
      }
    }

    if (!isAuthor && !isAdmin) {
      throw new SenderError('Can only delete your own messages or as admin');
    }

    // Delete associated data
    for (const reaction of ctx.db.messageReaction.by_message.filter(
      messageId
    )) {
      ctx.db.messageReaction.id.delete(reaction.id);
    }
    for (const receipt of ctx.db.readReceipt.by_message.filter(messageId)) {
      ctx.db.readReceipt.id.delete(receipt.id);
    }
    for (const edit of ctx.db.messageEdit.by_message.filter(messageId)) {
      ctx.db.messageEdit.id.delete(edit.id);
    }

    ctx.db.message.id.delete(messageId);
  }
);

// ============================================================================
// REACTIONS
// ============================================================================

spacetimedb.reducer(
  'toggle_reaction',
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    const validEmojis = ['👍', '❤️', '😂', '😮', '😢', '🎉', '🔥', '👀'];
    if (!validEmojis.includes(emoji)) {
      throw new SenderError('Invalid emoji');
    }

    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(message.roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    // Check if user already has this reaction
    for (const reaction of ctx.db.messageReaction.by_message.filter(
      messageId
    )) {
      if (
        reaction.userId.toHexString() === ctx.sender.toHexString() &&
        reaction.emoji === emoji
      ) {
        // Remove existing reaction
        ctx.db.messageReaction.id.delete(reaction.id);
        return;
      }
    }

    // Add new reaction
    ctx.db.messageReaction.insert({
      id: 0n,
      messageId,
      userId: ctx.sender,
      emoji,
      createdAt: ctx.timestamp,
    });
  }
);

// ============================================================================
// TYPING INDICATORS
// ============================================================================

spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  // Check membership
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

  // Create new typing indicator
  const typingId = ctx.db.typingIndicator.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    startedAt: ctx.timestamp,
  }).id;

  // Schedule expiry in 5 seconds
  const fiveSecs = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
  ctx.db.typingExpiry.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(fiveSecs),
    typingIndicatorId: typingId,
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

spacetimedb.reducer(
  'expire_typing',
  { arg: TypingExpiry.rowType },
  (ctx, { arg }) => {
    const typing = ctx.db.typingIndicator.id.find(arg.typingIndicatorId);
    if (typing) {
      // Only expire if not updated recently (within last 4 seconds)
      const fourSecsAgo = ctx.timestamp.microsSinceUnixEpoch - 4_000_000n;
      if (typing.startedAt.microsSinceUnixEpoch < fourSecsAgo) {
        ctx.db.typingIndicator.id.delete(typing.id);
      }
    }
  }
);

// ============================================================================
// READ RECEIPTS
// ============================================================================

spacetimedb.reducer(
  'mark_messages_read',
  { roomId: t.u64(), upToMessageId: t.u64() },
  (ctx, { roomId, upToMessageId }) => {
    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    // Get all messages in room up to upToMessageId
    const messages = [...ctx.db.message.by_room.filter(roomId)];

    for (const message of messages) {
      if (message.id <= upToMessageId) {
        // Check if already read
        let alreadyRead = false;
        for (const receipt of ctx.db.readReceipt.by_message.filter(
          message.id
        )) {
          if (receipt.userId.toHexString() === ctx.sender.toHexString()) {
            alreadyRead = true;
            break;
          }
        }

        if (!alreadyRead) {
          ctx.db.readReceipt.insert({
            id: 0n,
            messageId: message.id,
            roomId,
            userId: ctx.sender,
            readAt: ctx.timestamp,
          });
        }
      }
    }
  }
);

// ============================================================================
// SCHEDULED MESSAGES
// ============================================================================

spacetimedb.reducer(
  'schedule_message',
  { roomId: t.u64(), content: t.string(), sendAtTimestamp: t.u64() },
  (ctx, { roomId, content, sendAtTimestamp }) => {
    const trimmed = content.trim();
    if (!trimmed || trimmed.length > 2000) {
      throw new SenderError('Message must be 1-2000 characters');
    }

    // Check membership
    let isMember = false;
    for (const member of ctx.db.roomMember.by_room.filter(roomId)) {
      if (member.userId.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    // Validate timestamp is in the future
    if (sendAtTimestamp <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }

    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtTimestamp),
      roomId,
      senderId: ctx.sender,
      content: trimmed,
    });
  }
);

spacetimedb.reducer(
  'cancel_scheduled_message',
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const scheduled = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!scheduled) throw new SenderError('Scheduled message not found');

    if (scheduled.senderId.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Can only cancel your own scheduled messages');
    }

    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

spacetimedb.reducer(
  'send_scheduled_message',
  { arg: ScheduledMessage.rowType },
  (ctx, { arg }) => {
    // Verify user is still a member
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
        threadParentId: undefined,
        expiresAt: undefined,
      });
    }
    // Scheduled row is auto-deleted
  }
);
