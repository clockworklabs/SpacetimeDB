import { spacetimedb, ScheduledMessage, EphemeralMessage } from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// User management reducers
spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  if (!name.trim()) throw new SenderError('Name cannot be empty');
  if (name.length > 50) throw new SenderError('Name too long');

  const existingUser = ctx.db.user.identity.find(ctx.sender);
  if (existingUser) {
    ctx.db.user.identity.update({
      ...existingUser,
      name: name.trim(),
      online: true,
      lastSeen: ctx.timestamp,
    });
  } else {
    ctx.db.user.insert({
      id: 0n,
      identity: ctx.sender,
      name: name.trim(),
      online: true,
      lastSeen: ctx.timestamp,
    });
  }
});

// Room management reducers
spacetimedb.reducer(
  'create_room',
  { name: t.string(), description: t.string().optional() },
  (ctx, { name, description }) => {
    if (!name.trim()) throw new SenderError('Room name cannot be empty');
    if (name.length > 100) throw new SenderError('Room name too long');

    // Check if user exists
    let user = ctx.db.user.identity.find(ctx.sender);
    if (!user) {
      user = ctx.db.user.insert({
        id: 0n,
        identity: ctx.sender,
        name: `User_${ctx.sender.toHexString().slice(0, 8)}`,
        online: true,
        lastSeen: ctx.timestamp,
      });
    }

    const room = ctx.db.room.insert({
      id: 0n,
      name: name.trim(),
      description: description?.trim(),
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });

    // Add creator as owner
    ctx.db.roomMember.insert({
      id: 0n,
      roomId: room.id,
      identity: ctx.sender,
      role: 'owner',
      joinedAt: ctx.timestamp,
    });

    // Initialize read position for creator
    ctx.db.roomReadPosition.insert({
      id: 0n,
      roomId: room.id,
      userId: ctx.sender,
      lastReadMessageId: 0n,
      lastReadAt: ctx.timestamp,
    });
  }
);

spacetimedb.reducer('join_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');

  const existingMember = ctx.db.roomMember.room_identity
    .filter(roomId)
    .find(member => member.identity.toHexString() === ctx.sender.toHexString());
  if (existingMember) throw new SenderError('Already a member of this room');

  // Ensure user exists
  let user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    user = ctx.db.user.insert({
      id: 0n,
      identity: ctx.sender,
      name: `User_${ctx.sender.toHexString().slice(0, 8)}`,
      online: true,
      lastSeen: ctx.timestamp,
    });
  }

  ctx.db.roomMember.insert({
    id: 0n,
    roomId,
    identity: ctx.sender,
    role: 'member',
    joinedAt: ctx.timestamp,
  });

  // Initialize read position
  ctx.db.roomReadPosition.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    lastReadMessageId: 0n,
    lastReadAt: ctx.timestamp,
  });
});

spacetimedb.reducer('leave_room', { roomId: t.u64() }, (ctx, { roomId }) => {
  const member = ctx.db.roomMember.room_identity
    .filter(roomId)
    .find(member => member.identity.toHexString() === ctx.sender.toHexString());
  if (!member) throw new SenderError('Not a member of this room');

  // Remove all typing indicators for this user in this room
  for (const indicator of ctx.db.typingIndicator.room_user.filter(roomId)) {
    if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }

  // Remove read position
  const readPos = ctx.db.roomReadPosition.room_user
    .filter(roomId)
    .find(pos => pos.userId.toHexString() === ctx.sender.toHexString());
  if (readPos) {
    ctx.db.roomReadPosition.id.delete(readPos.id);
  }

  // Remove membership
  ctx.db.roomMember.id.delete(member.id);
});

// Message management reducers
spacetimedb.reducer(
  'send_message',
  { roomId: t.u64(), content: t.string() },
  (ctx, { roomId, content }) => {
    if (!content.trim()) throw new SenderError('Message cannot be empty');
    if (content.length > 2000) throw new SenderError('Message too long');

    const member = ctx.db.roomMember.room_identity
      .filter(roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('User not found');

    const message = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderId: ctx.sender,
      senderName: user.name,
      content: content.trim(),
      sentAt: ctx.timestamp,
      isEphemeral: false,
      ephemeralExpiresAt: undefined,
    });

    // Remove typing indicator if user was typing
    for (const indicator of ctx.db.typingIndicator.room_user.filter(roomId)) {
      if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }

    return message;
  }
);

spacetimedb.reducer(
  'edit_message',
  { messageId: t.u64(), newContent: t.string() },
  (ctx, { messageId, newContent }) => {
    if (!newContent.trim()) throw new SenderError('Message cannot be empty');
    if (newContent.length > 2000) throw new SenderError('Message too long');

    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    if (message.senderId.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Can only edit your own messages');
    }

    // Store edit history
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      previousContent: message.content,
      newContent: newContent.trim(),
      editedAt: ctx.timestamp,
      editedBy: ctx.sender,
    });

    // Update message
    ctx.db.message.id.update({
      ...message,
      content: newContent.trim(),
      editedAt: ctx.timestamp,
    });
  }
);

spacetimedb.reducer(
  'delete_message',
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    const member = ctx.db.roomMember.room_identity
      .filter(message.roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not authorized');

    // Only allow deletion by message author or room admin/owner
    const isAuthor =
      message.senderId.toHexString() === ctx.sender.toHexString();
    const canModerate = member.role === 'owner' || member.role === 'admin';

    if (!isAuthor && !canModerate) {
      throw new SenderError(
        'Can only delete your own messages or moderate as admin/owner'
      );
    }

    // Remove all reactions to this message
    for (const reaction of ctx.db.reaction.message_id.filter(messageId)) {
      ctx.db.reaction.id.delete(reaction.id);
    }

    // Remove all read receipts for this message
    for (const receipt of ctx.db.readReceipt.message_id.filter(messageId)) {
      ctx.db.readReceipt.id.delete(receipt.id);
    }

    // Remove edit history
    for (const edit of ctx.db.messageEdit.message_id.filter(messageId)) {
      ctx.db.messageEdit.id.delete(edit.id);
    }

    // Remove ephemeral cleanup if exists
    for (const ephemeral of ctx.db.ephemeralMessage.message_id.filter(
      messageId
    )) {
      ctx.db.ephemeralMessage.scheduledId.delete(ephemeral.scheduledId);
    }

    // Delete the message
    ctx.db.message.id.delete(messageId);
  }
);

// Reaction reducers
spacetimedb.reducer(
  'toggle_reaction',
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    // Check if user is member of the room
    const member = ctx.db.roomMember.room_identity
      .filter(message.roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    const existingReaction = ctx.db.reaction.message_user
      .filter(messageId)
      .find(
        reaction =>
          reaction.userId.toHexString() === ctx.sender.toHexString() &&
          reaction.emoji === emoji
      );

    if (existingReaction) {
      // Remove reaction
      ctx.db.reaction.id.delete(existingReaction.id);
    } else {
      // Add reaction
      ctx.db.reaction.insert({
        id: 0n,
        messageId,
        userId: ctx.sender,
        emoji,
        reactedAt: ctx.timestamp,
      });
    }
  }
);

// Read receipt reducers
spacetimedb.reducer(
  'mark_message_read',
  { messageId: t.u64() },
  (ctx, { messageId }) => {
    const message = ctx.db.message.id.find(messageId);
    if (!message) throw new SenderError('Message not found');

    // Check if user is member of the room
    const member = ctx.db.roomMember.room_identity
      .filter(message.roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    // Check if already read
    const existingReceipt = ctx.db.readReceipt.user_message
      .filter(ctx.sender.toHexString())
      .find(receipt => receipt.messageId === messageId);

    if (!existingReceipt) {
      ctx.db.readReceipt.insert({
        id: 0n,
        messageId,
        userId: ctx.sender,
        readAt: ctx.timestamp,
      });
    }

    // Update room read position
    const readPos = ctx.db.roomReadPosition.room_user
      .filter(message.roomId)
      .find(pos => pos.userId.toHexString() === ctx.sender.toHexString());

    if (readPos && messageId > readPos.lastReadMessageId) {
      ctx.db.roomReadPosition.id.update({
        ...readPos,
        lastReadMessageId: messageId,
        lastReadAt: ctx.timestamp,
      });
    }
  }
);

spacetimedb.reducer(
  'mark_room_read',
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const member = ctx.db.roomMember.room_identity
      .filter(roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    // Find the latest message in the room
    let latestMessageId = 0n;
    for (const message of ctx.db.message.room_id.filter(roomId)) {
      if (message.id > latestMessageId) {
        latestMessageId = message.id;
      }
    }

    const readPos = ctx.db.roomReadPosition.room_user
      .filter(roomId)
      .find(pos => pos.userId.toHexString() === ctx.sender.toHexString());

    if (readPos) {
      ctx.db.roomReadPosition.id.update({
        ...readPos,
        lastReadMessageId: latestMessageId,
        lastReadAt: ctx.timestamp,
      });
    }

    // Mark all messages as read
    for (const message of ctx.db.message.room_id.filter(roomId)) {
      const existingReceipt = ctx.db.readReceipt.user_message
        .filter(ctx.sender.toHexString())
        .find(receipt => receipt.messageId === message.id);
      if (!existingReceipt) {
        ctx.db.readReceipt.insert({
          id: 0n,
          messageId: message.id,
          userId: ctx.sender,
          readAt: ctx.timestamp,
        });
      }
    }
  }
);

// Typing indicator reducers
spacetimedb.reducer('start_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  const member = ctx.db.roomMember.room_identity
    .filter(roomId)
    .find(member => member.identity.toHexString() === ctx.sender.toHexString());
  if (!member) throw new SenderError('Not a member of this room');

  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) throw new SenderError('User not found');

  // Remove existing typing indicator for this user in this room
  for (const indicator of ctx.db.typingIndicator.room_user.filter(roomId)) {
    if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }

  // Add new typing indicator
  ctx.db.typingIndicator.insert({
    id: 0n,
    roomId,
    userId: ctx.sender,
    userName: user.name,
    startedTypingAt: ctx.timestamp,
  });
});

spacetimedb.reducer('stop_typing', { roomId: t.u64() }, (ctx, { roomId }) => {
  // Remove typing indicator for this user in this room
  for (const indicator of ctx.db.typingIndicator.room_user.filter(roomId)) {
    if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }
});

// Scheduled message reducers
spacetimedb.reducer(
  'schedule_message',
  { roomId: t.u64(), content: t.string(), delaySeconds: t.u64() },
  (ctx, { roomId, content, delaySeconds }) => {
    if (!content.trim()) throw new SenderError('Message cannot be empty');
    if (content.length > 2000) throw new SenderError('Message too long');
    if (delaySeconds < 10 || delaySeconds > 86400)
      throw new SenderError('Delay must be between 10 seconds and 24 hours');

    const member = ctx.db.roomMember.room_identity
      .filter(roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('User not found');

    const scheduledAt =
      ctx.timestamp.microsSinceUnixEpoch + BigInt(delaySeconds) * 1000000n;

    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(scheduledAt),
      roomId,
      senderId: ctx.sender,
      senderName: user.name,
      content: content.trim(),
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

// Ephemeral message reducers
spacetimedb.reducer(
  'send_ephemeral_message',
  { roomId: t.u64(), content: t.string(), durationSeconds: t.u64() },
  (ctx, { roomId, content, durationSeconds }) => {
    if (!content.trim()) throw new SenderError('Message cannot be empty');
    if (content.length > 2000) throw new SenderError('Message too long');
    if (durationSeconds < 10 || durationSeconds > 3600)
      throw new SenderError('Duration must be between 10 seconds and 1 hour');

    const member = ctx.db.roomMember.room_identity
      .filter(roomId)
      .find(
        member => member.identity.toHexString() === ctx.sender.toHexString()
      );
    if (!member) throw new SenderError('Not a member of this room');

    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new SenderError('User not found');

    const expiresAt =
      ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSeconds) * 1000000n;

    const message = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderId: ctx.sender,
      senderName: user.name,
      content: content.trim(),
      sentAt: ctx.timestamp,
      isEphemeral: true,
      ephemeralExpiresAt: { microsSinceUnixEpoch: expiresAt },
    });

    // Schedule cleanup
    ctx.db.ephemeralMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAt),
      messageId: message.id,
    });

    // Remove typing indicator if user was typing
    for (const indicator of ctx.db.typingIndicator.room_user.filter(roomId)) {
      if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(indicator.id);
      }
    }

    return message;
  }
);

// Lifecycle hooks
spacetimedb.clientConnected(ctx => {
  let user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: true,
      lastSeen: ctx.timestamp,
    });
  } else {
    user = ctx.db.user.insert({
      id: 0n,
      identity: ctx.sender,
      name: `User_${ctx.sender.toHexString().slice(0, 8)}`,
      online: true,
      lastSeen: ctx.timestamp,
    });
  }
});

spacetimedb.clientDisconnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({
      ...user,
      online: false,
      lastSeen: ctx.timestamp,
    });
  }

  // Remove all typing indicators for this user
  for (const indicator of ctx.db.typingIndicator.iter()) {
    if (indicator.userId.toHexString() === ctx.sender.toHexString()) {
      ctx.db.typingIndicator.id.delete(indicator.id);
    }
  }
});

// Scheduled reducer implementations
spacetimedb.reducer(
  'send_scheduled_message',
  { arg: ScheduledMessage.rowType },
  (ctx, { arg }) => {
    // Insert the message
    ctx.db.message.insert({
      id: 0n,
      roomId: arg.roomId,
      senderId: arg.senderId,
      senderName: arg.senderName,
      content: arg.content,
      sentAt: ctx.timestamp,
      isEphemeral: false,
      ephemeralExpiresAt: undefined,
    });

    // The scheduled message row is auto-deleted after this reducer completes
  }
);

spacetimedb.reducer(
  'cleanup_ephemeral_message',
  { arg: EphemeralMessage.rowType },
  (ctx, { arg }) => {
    // Delete the message and all related data
    const message = ctx.db.message.id.find(arg.messageId);
    if (message) {
      // Remove reactions
      for (const reaction of ctx.db.reaction.message_id.filter(arg.messageId)) {
        ctx.db.reaction.id.delete(reaction.id);
      }

      // Remove read receipts
      for (const receipt of ctx.db.readReceipt.message_id.filter(
        arg.messageId
      )) {
        ctx.db.readReceipt.id.delete(receipt.id);
      }

      // Remove edit history
      for (const edit of ctx.db.messageEdit.message_id.filter(arg.messageId)) {
        ctx.db.messageEdit.id.delete(edit.id);
      }

      // Delete the message
      ctx.db.message.id.delete(arg.messageId);
    }

    // The ephemeral message row is auto-deleted after this reducer completes
  }
);
