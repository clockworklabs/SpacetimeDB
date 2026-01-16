import { spacetimedb, ScheduledMessage, EphemeralMessage } from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { Timestamp, ScheduleAt } from 'spacetimedb';

// User Management Reducers
spacetimedb.reducer('set_display_name', { displayName: t.string() }, (ctx, { displayName }) => {
  if (!displayName.trim()) throw new SenderError('Display name cannot be empty');
  if (displayName.length > 50) throw new SenderError('Display name too long (max 50 chars)');

  const existingUser = ctx.db.user.identity.find(ctx.sender);
  if (existingUser) {
    ctx.db.user.identity.update({
      ...existingUser,
      displayName: displayName.trim(),
      lastSeen: ctx.timestamp
    });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      displayName: displayName.trim(),
      createdAt: ctx.timestamp,
      lastSeen: ctx.timestamp,
      isOnline: true
    });
  }

  // Update user status
  const existingStatus = ctx.db.userStatus.identity.find(ctx.sender);
  if (existingStatus) {
    ctx.db.userStatus.identity.update({
      ...existingStatus,
      isOnline: true,
      lastSeen: ctx.timestamp
    });
  } else {
    ctx.db.userStatus.insert({
      identity: ctx.sender,
      isOnline: true,
      lastSeen: ctx.timestamp
    });
  }
});

// Room Management Reducers
spacetimedb.reducer('create_room', { name: t.string(), description: t.string().optional(), isPublic: t.bool() }, (ctx, { name, description, isPublic }) => {
  if (!name.trim()) throw new SenderError('Room name cannot be empty');
  if (name.length > 100) throw new SenderError('Room name too long (max 100 chars)');
  if (description && description.length > 500) throw new SenderError('Description too long (max 500 chars)');

  // Ensure user exists
  let user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    user = ctx.db.user.insert({
      identity: ctx.sender,
      displayName: `User_${ctx.sender.toHexString().slice(0, 8)}`,
      createdAt: ctx.timestamp,
      lastSeen: ctx.timestamp,
      isOnline: true
    });
  }

  const room = ctx.db.room.insert({
    id: 0n,
    name: name.trim(),
    description: description?.trim(),
    ownerId: ctx.sender,
    createdAt: ctx.timestamp,
    isPublic
  });

  // Add creator as member
  ctx.db.roomMember.insert({
    id: 0n,
    roomId: room.id,
    userId: ctx.sender,
    joinedAt: ctx.timestamp,
    lastReadMessageId: null
  });
});

spacetimedb.reducer('join_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');

  if (!room.isPublic) {
    // For private rooms, we could add invite logic here
    throw new SenderError('This room is private');
  }

  // Check if already a member
  const existingMember = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .find(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!existingMember) {
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      userId: ctx.sender,
      joinedAt: ctx.timestamp,
      lastReadMessageId: null
    });
  }
});

spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const member = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .find(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (member) {
    ctx.db.roomMember.id.delete(member.id);
  }
});

// Message Reducers
spacetimedb.reducer('send_message', { roomId: t.u64(), content: t.string() }, (ctx, { roomId, content }) => {
  if (!content.trim()) throw new SenderError('Message cannot be empty');
  if (content.length > 2000) throw new SenderError('Message too long (max 2000 chars)');

  // Verify user is member of room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) throw new SenderError('You are not a member of this room');

  // Rate limiting: max 5 messages per minute per user per room
  const oneMinuteAgo = ctx.timestamp.microsSinceUnixEpoch - 60_000_000n;
  const recentMessages = [...ctx.db.message.message_room_id.filter(roomId)]
    .filter(msg =>
      msg.authorId.toHexString() === ctx.sender.toHexString() &&
      msg.createdAt.microsSinceUnixEpoch > oneMinuteAgo
    );

  if (recentMessages.length >= 5) {
    throw new SenderError('Rate limit exceeded. Please wait before sending more messages.');
  }

  const message = ctx.db.message.insert({
    id: 0n,
    roomId,
    authorId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
    editedAt: null,
    isEdited: false
  });

  // Clear typing indicator
  const typingIndicator = [...ctx.db.typingIndicator.typing_indicator_room_id.filter(roomId)]
    .find(indicator => indicator.userId.toHexString() === ctx.sender.toHexString());

  if (typingIndicator) {
    ctx.db.typingIndicator.id.delete(typingIndicator.id);
  }

  // Mark as read by sender
  ctx.db.readReceipt.insert({
    id: 0n,
    messageId: message.id,
    userId: ctx.sender,
    readAt: ctx.timestamp
  });
});

// Message Editing with History
spacetimedb.reducer('edit_message', { messageId: t.u64(), newContent: t.string() }, (ctx, { messageId, newContent }) => {
  if (!newContent.trim()) throw new SenderError('Message cannot be empty');
  if (newContent.length > 2000) throw new SenderError('Message too long (max 2000 chars)');

  const message = ctx.db.message.id.find(messageId);
  if (!message) throw new SenderError('Message not found');

  // Only author can edit their own messages
  if (message.authorId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('You can only edit your own messages');
  }

  // Don't allow editing messages older than 5 minutes
  const fiveMinutesAgo = ctx.timestamp.microsSinceUnixEpoch - 300_000_000n;
  if (message.createdAt.microsSinceUnixEpoch < fiveMinutesAgo) {
    throw new SenderError('Messages can only be edited within 5 minutes of sending');
  }

  // Store edit history
  ctx.db.messageEdit.insert({
    id: 0n,
    messageId,
    previousContent: message.content,
    newContent: newContent.trim(),
    editedAt: ctx.timestamp,
    editedBy: ctx.sender
  });

  // Update the message
  ctx.db.message.id.update({
    ...message,
    content: newContent.trim(),
    editedAt: ctx.timestamp,
    isEdited: true
  });
});

// Scheduled Messages
spacetimedb.reducer('schedule_message', { roomId: t.u64(), content: t.string(), delayMinutes: t.u64() }, (ctx, { roomId, content, delayMinutes }) => {
  if (!content.trim()) throw new SenderError('Message cannot be empty');
  if (content.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
  if (delayMinutes < 1 || delayMinutes > 1440) throw new SenderError('Delay must be between 1 and 1440 minutes (24 hours)');

  // Verify user is member of room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) throw new SenderError('You are not a member of this room');

  const scheduledTime = ctx.timestamp.microsSinceUnixEpoch + (delayMinutes * 60_000_000n);

  ctx.db.scheduledMessage.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(scheduledTime),
    roomId,
    authorId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp
  });
});

spacetimedb.reducer('cancel_scheduled_message', { scheduledId: t.u64() }, (ctx, { scheduledId }) => {
  const scheduled = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
  if (!scheduled) throw new SenderError('Scheduled message not found');

  // Only author can cancel
  if (scheduled.authorId.toHexString() !== ctx.sender.toHexString()) {
    throw new SenderError('You can only cancel your own scheduled messages');
  }

  ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
});


// Ephemeral Messages
spacetimedb.reducer('send_ephemeral_message', { roomId: t.u64(), content: t.string(), durationMinutes: t.u64() }, (ctx, { roomId, content, durationMinutes }) => {
  if (!content.trim()) throw new SenderError('Message cannot be empty');
  if (content.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
  if (durationMinutes < 1 || durationMinutes > 60) throw new SenderError('Duration must be between 1 and 60 minutes');

  // Verify user is member of room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) throw new SenderError('You are not a member of this room');

  // First insert the regular message
  const message = ctx.db.message.insert({
    id: 0n,
    roomId,
    authorId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
    editedAt: null,
    isEdited: false
  });

  // Then schedule its deletion
  const deleteTime = ctx.timestamp.microsSinceUnixEpoch + (durationMinutes * 60_000_000n);

  ctx.db.ephemeralMessage.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(deleteTime),
    messageId: message.id,
    roomId,
    authorId: ctx.sender,
    content: content.trim(),
    createdAt: ctx.timestamp,
    durationMinutes
  });

  // Mark as read by sender
  ctx.db.readReceipt.insert({
    id: 0n,
    messageId: message.id,
    userId: ctx.sender,
    readAt: ctx.timestamp
  });
});


// Typing Indicators
spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  // Verify user is member of room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) return; // Silently ignore if not a member

  // Remove existing typing indicator for this user in this room
  const existing = [...ctx.db.typingIndicator.typing_indicator_room_id.filter(roomId)]
    .find(indicator => indicator.userId.toHexString() === ctx.sender.toHexString());

  if (existing) {
    ctx.db.typingIndicator.id.update({
      ...existing,
      startedAt: ctx.timestamp
    });
  } else {
    ctx.db.typingIndicator.insert({
      id: 0n,
      roomId,
      userId: ctx.sender,
      startedAt: ctx.timestamp
    });
  }
});

spacetimedb.reducer('stop_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  const typingIndicator = [...ctx.db.typingIndicator.typing_indicator_room_id.filter(roomId)]
    .find(indicator => indicator.userId.toHexString() === ctx.sender.toHexString());

  if (typingIndicator) {
    ctx.db.typingIndicator.id.delete(typingIndicator.id);
  }
});

// Read Receipts
spacetimedb.reducer('mark_message_read', { messageId: t.u64() }, (ctx, { messageId }) => {
  const message = ctx.db.message.id.find(messageId);
  if (!message) return; // Silently ignore if message doesn't exist

  // Check if user is member of the room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(message.roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) return; // Silently ignore if not a member

  // Check if already marked as read
  const existing = [...ctx.db.readReceipt.read_receipt_message_id.filter(messageId)]
    .find(receipt => receipt.userId.toHexString() === ctx.sender.toHexString());

  if (!existing) {
    ctx.db.readReceipt.insert({
      id: 0n,
      messageId,
      userId: ctx.sender,
      readAt: ctx.timestamp
    });

    // Update last read message for room membership
    const membership = [...ctx.db.roomMember.room_member_room_id.filter(message.roomId)]
      .find(member => member.userId.toHexString() === ctx.sender.toHexString());

    if (membership) {
      ctx.db.roomMember.id.update({
        ...membership,
        lastReadMessageId: messageId
      });
    }
  }
});

// Message Reactions
spacetimedb.reducer('toggle_reaction', { messageId: t.u64(), emoji: t.string() }, (ctx, { messageId, emoji }) => {
  if (!emoji.trim() || emoji.length > 10) throw new SenderError('Invalid emoji');

  const message = ctx.db.message.id.find(messageId);
  if (!message) throw new SenderError('Message not found');

  // Check if user is member of the room
  const isMember = [...ctx.db.roomMember.room_member_room_id.filter(message.roomId)]
    .some(member => member.userId.toHexString() === ctx.sender.toHexString());

  if (!isMember) throw new SenderError('You are not a member of this room');

  // Check if reaction already exists
  const existingReaction = [...ctx.db.messageReaction.message_reaction_message_id.filter(messageId)]
    .find(reaction =>
      reaction.userId.toHexString() === ctx.sender.toHexString() &&
      reaction.emoji === emoji
    );

  if (existingReaction) {
    // Remove reaction
    ctx.db.messageReaction.id.delete(existingReaction.id);
  } else {
    // Add reaction
    ctx.db.messageReaction.insert({
      id: 0n,
      messageId,
      userId: ctx.sender,
      emoji: emoji.trim(),
      reactedAt: ctx.timestamp
    });
  }
});

// Lifecycle hooks
spacetimedb.clientConnected((ctx) => {
  // Update user status to online
  const existingStatus = ctx.db.userStatus.identity.find(ctx.sender);
  if (existingStatus) {
    ctx.db.userStatus.identity.update({
      ...existingStatus,
      isOnline: true,
      lastSeen: ctx.timestamp
    });
  } else {
    ctx.db.userStatus.insert({
      identity: ctx.sender,
      isOnline: true,
      lastSeen: ctx.timestamp
    });
  }

  // Create user if doesn't exist
  const existingUser = ctx.db.user.identity.find(ctx.sender);
  if (!existingUser) {
    ctx.db.user.insert({
      identity: ctx.sender,
      displayName: `User_${ctx.sender.toHexString().slice(0, 8)}`,
      createdAt: ctx.timestamp,
      lastSeen: ctx.timestamp,
      isOnline: true
    });
  } else {
    ctx.db.user.identity.update({
      ...existingUser,
      lastSeen: ctx.timestamp,
      isOnline: true
    });
  }
});

spacetimedb.clientDisconnected((ctx) => {
  // Update user status to offline
  const existingStatus = ctx.db.userStatus.identity.find(ctx.sender);
  if (existingStatus) {
    ctx.db.userStatus.identity.update({
      ...existingStatus,
      isOnline: false,
      lastSeen: ctx.timestamp
    });
  }

  // Clear typing indicators for this user
  for (const indicator of ctx.db.typingIndicator.typing_indicator_user_id.filter(ctx.sender)) {
    ctx.db.typingIndicator.id.delete(indicator.id);
  }
});

// Scheduled reducers (defined after schema to reference table types)
spacetimedb.reducer('send_scheduled_message', { arg: ScheduledMessage.rowType }, (ctx, { arg }) => {
  // Insert the message into the regular messages table
  const message = ctx.db.message.insert({
    id: 0n,
    roomId: arg.roomId,
    authorId: arg.authorId,
    content: arg.content,
    createdAt: ctx.timestamp,
    editedAt: null,
    isEdited: false
  });

  // Mark as read by sender
  ctx.db.readReceipt.insert({
    id: 0n,
    messageId: message.id,
    userId: arg.authorId,
    readAt: ctx.timestamp
  });

  // The scheduled message row will be auto-deleted after this reducer completes
});

spacetimedb.reducer('delete_ephemeral_message', { arg: EphemeralMessage.rowType }, (ctx, { arg }) => {
  // Delete the message
  ctx.db.message.id.delete(arg.messageId);
  // The ephemeral message row will be auto-deleted after this reducer completes
});