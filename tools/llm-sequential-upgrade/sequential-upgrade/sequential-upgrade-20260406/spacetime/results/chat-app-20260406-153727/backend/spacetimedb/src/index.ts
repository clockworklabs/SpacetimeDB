import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
export { default } from './schema';
export { sendScheduledMessage, deleteExpiredMessage } from './schema';

// Lifecycle hooks
export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    // Restore to online unless user explicitly set invisible
    const newStatus = existing.status === 'invisible' ? 'invisible' : 'online';
    ctx.db.user.identity.update({ ...existing, status: newStatus, lastActiveAt: null });
  } else {
    // Auto-create an anonymous user with a temporary name derived from their identity
    const hex = ctx.sender.toHexString();
    const shortId = hex.slice(0, 6);
    ctx.db.user.insert({
      identity: ctx.sender,
      name: `Anon_${shortId}`,
      status: 'online',
      lastActiveAt: null,
      createdAt: ctx.timestamp,
      isAnonymous: true,
    });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, status: 'offline', lastActiveAt: ctx.timestamp });
    // Clear typing indicators for this user
    for (const ti of [...ctx.db.typingIndicator.userIdentity.filter(ctx.sender)]) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }
  }
});

// Set or update display name (also marks user as registered, no longer anonymous)
export const setName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 chars)');

    const existing = ctx.db.user.identity.find(ctx.sender);
    if (existing) {
      ctx.db.user.identity.update({ ...existing, name: trimmed, isAnonymous: false });
    } else {
      ctx.db.user.insert({ identity: ctx.sender, name: trimmed, status: 'online', lastActiveAt: null, createdAt: ctx.timestamp, isAnonymous: false });
    }
  }
);

// Set user presence status
export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');

    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('User not found');
    ctx.db.user.identity.update({ ...existing, status, lastActiveAt: ctx.timestamp });
  }
);

// Create a room
export const createRoom = spacetimedb.reducer(
  { name: t.string(), isPrivate: t.bool() },
  (ctx, { name, isPrivate }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 chars)');

    // Check for duplicate name
    const existing = ctx.db.room.name.find(trimmed);
    if (existing) throw new SenderError('Room already exists');

    const roomId = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate, isDm: false }).id;
    // Auto-join and auto-admin the creator
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
    ctx.db.roomAdmin.insert({ id: 0n, roomId, userIdentity: ctx.sender });
  }
);

// Join a room (public only — private rooms require invitation via acceptInvitation)
export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    if (room.isPrivate) throw new SenderError('This is a private room. You must be invited.');

    // Check if banned
    for (const b of [...ctx.db.bannedUser.roomId.filter(roomId)]) {
      if (b.userIdentity.toHexString() === ctx.sender.toHexString()) {
        throw new SenderError('You are banned from this room');
      }
    }

    // Check if already a member
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        throw new SenderError('Already a member');
      }
    }
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
  }
);

// Leave a room
export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
        // Clear typing indicator if any
        for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
          if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
            ctx.db.typingIndicator.id.delete(ti.id);
          }
        }
        return;
      }
    }
    throw new SenderError('Not a member of this room');
  }
);

// Send a message
export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Verify membership
    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');

    const msg = ctx.db.message.insert({ id: 0n, roomId, senderIdentity: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAt: null, editedAt: null, parentMessageId: null });

    // Update sender's read receipt to this message
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }
    if (found) {
      ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    }

    // Clear typing indicator for sender in this room
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }
  }
);

// Send an ephemeral message that auto-deletes after durationSecs seconds
export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), durationSecs: t.u32() },
  (ctx, { roomId, text, durationSecs }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (durationSecs < 1 || durationSecs > 86400) throw new SenderError('Invalid duration');

    const expiryMicros = ctx.timestamp.microsSinceUnixEpoch + BigInt(durationSecs) * 1_000_000n;

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: new Timestamp(expiryMicros),
      editedAt: null,
      parentMessageId: null,
    });

    // Update sender's read receipt
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }
    if (found) {
      ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    }

    // Clear typing indicator
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }

    // Schedule deletion
    ctx.db.messageExpiry.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiryMicros),
      messageId: msg.id,
    });
  }
);

// Edit a message and save previous version to history
export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (msg.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Can only edit your own messages');
    }

    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (trimmed === msg.text) return; // No change

    // Save previous version to history
    ctx.db.messageEdit.insert({ id: 0n, messageId, previousText: msg.text, editedAt: ctx.timestamp });

    // Update the message
    ctx.db.message.id.update({ ...msg, text: trimmed, editedAt: ctx.timestamp });
  }
);

// Set typing indicator
export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    if (!ctx.db.room.id.find(roomId)) return;

    // Find existing
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = ti;
        break;
      }
    }

    if (isTyping) {
      if (found) {
        ctx.db.typingIndicator.id.update({ ...found, updatedAt: ctx.timestamp });
      } else {
        ctx.db.typingIndicator.insert({ id: 0n, roomId, userIdentity: ctx.sender, updatedAt: ctx.timestamp });
      }
    } else {
      if (found) {
        ctx.db.typingIndicator.id.delete(found.id);
      }
    }
  }
);

// Mark messages as read up to a given message ID
export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;

    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(roomId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = r;
        break;
      }
    }

    if (found) {
      if (messageId > found.lastReadMessageId) {
        ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: messageId, updatedAt: ctx.timestamp });
      }
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId, userIdentity: ctx.sender, lastReadMessageId: messageId, updatedAt: ctx.timestamp });
    }
  }
);

// Schedule a message to be sent at a future time
export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), scheduledAtMicros: t.u64() },
  (ctx, { roomId, text, scheduledAtMicros }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Message too long (max 2000 chars)');
    if (scheduledAtMicros <= ctx.timestamp.microsSinceUnixEpoch) {
      throw new SenderError('Scheduled time must be in the future');
    }

    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(scheduledAtMicros),
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
    });
  }
);

// Cancel a pending scheduled message
export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const row = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!row) throw new SenderError('Scheduled message not found');
    if (row.senderIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Not your scheduled message');
    }
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// Kick a user from a room (removes them and bans from rejoining)
export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), target: t.identity() },
  (ctx, { roomId, target }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Check caller is admin (check roomAdmin table or is room creator)
    let callerIsAdmin = false;
    for (const a of [...ctx.db.roomAdmin.roomId.filter(roomId)]) {
      if (a.userIdentity.toHexString() === ctx.sender.toHexString()) { callerIsAdmin = true; break; }
    }
    if (!callerIsAdmin) {
      const room = ctx.db.room.id.find(roomId);
      if (room && room.createdBy.toHexString() === ctx.sender.toHexString()) callerIsAdmin = true;
    }
    if (!callerIsAdmin) throw new SenderError('Not authorized');

    // Cannot kick an admin
    let targetIsAdmin = false;
    for (const a of [...ctx.db.roomAdmin.roomId.filter(roomId)]) {
      if (a.userIdentity.toHexString() === target.toHexString()) { targetIsAdmin = true; break; }
    }
    if (!targetIsAdmin) {
      const room = ctx.db.room.id.find(roomId);
      if (room && room.createdBy.toHexString() === target.toHexString()) targetIsAdmin = true;
    }
    if (targetIsAdmin) throw new SenderError('Cannot kick an admin');

    // Remove from room membership
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === target.toHexString()) {
        ctx.db.roomMember.id.delete(m.id);
        break;
      }
    }

    // Clear typing indicators
    for (const ti of [...ctx.db.typingIndicator.roomId.filter(roomId)]) {
      if (ti.userIdentity.toHexString() === target.toHexString()) {
        ctx.db.typingIndicator.id.delete(ti.id);
      }
    }

    // Add to banned list (prevent rejoin)
    let alreadyBanned = false;
    for (const b of [...ctx.db.bannedUser.roomId.filter(roomId)]) {
      if (b.userIdentity.toHexString() === target.toHexString()) { alreadyBanned = true; break; }
    }
    if (!alreadyBanned) {
      ctx.db.bannedUser.insert({ id: 0n, roomId, userIdentity: target });
    }
  }
);

// Promote a room member to admin
export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), target: t.identity() },
  (ctx, { roomId, target }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Check caller is admin
    let callerIsAdmin = false;
    for (const a of [...ctx.db.roomAdmin.roomId.filter(roomId)]) {
      if (a.userIdentity.toHexString() === ctx.sender.toHexString()) { callerIsAdmin = true; break; }
    }
    if (!callerIsAdmin) {
      const room = ctx.db.room.id.find(roomId);
      if (room && room.createdBy.toHexString() === ctx.sender.toHexString()) callerIsAdmin = true;
    }
    if (!callerIsAdmin) throw new SenderError('Not authorized');

    // Target must be a member
    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === target.toHexString()) { isMember = true; break; }
    }
    if (!isMember) throw new SenderError('User is not a member of this room');

    // Promote if not already admin
    let alreadyAdmin = false;
    for (const a of [...ctx.db.roomAdmin.roomId.filter(roomId)]) {
      if (a.userIdentity.toHexString() === target.toHexString()) { alreadyAdmin = true; break; }
    }
    if (!alreadyAdmin) {
      ctx.db.roomAdmin.insert({ id: 0n, roomId, userIdentity: target });
    }
  }
);

// Reply to a message, creating a thread
export const replyToMessage = spacetimedb.reducer(
  { parentMessageId: t.u64(), text: t.string() },
  (ctx, { parentMessageId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    const parentMsg = ctx.db.message.id.find(parentMessageId);
    if (!parentMsg) throw new SenderError('Parent message not found');

    const roomId = parentMsg.roomId;
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');

    // Verify membership
    let isMember = false;
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === ctx.sender.toHexString()) {
        isMember = true;
        break;
      }
    }
    if (!isMember) throw new SenderError('Not a member of this room');

    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Reply cannot be empty');
    if (trimmed.length > 2000) throw new SenderError('Reply too long (max 2000 chars)');

    ctx.db.message.insert({
      id: 0n,
      roomId,
      senderIdentity: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
      expiresAt: null,
      editedAt: null,
      parentMessageId,
    });
  }
);

// Toggle a reaction on a message (add if not present, remove if already reacted with same emoji)
export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');

    const validEmojis = ['👍', '❤️', '😂', '😮', '😢'];
    if (!validEmojis.includes(emoji)) throw new SenderError('Invalid emoji');

    // Check if user already reacted with this emoji
    let found: { id: bigint; messageId: bigint; userIdentity: { toHexString(): string }; emoji: string } | undefined;
    for (const r of [...ctx.db.messageReaction.messageId.filter(messageId)]) {
      if (r.userIdentity.toHexString() === ctx.sender.toHexString() && r.emoji === emoji) {
        found = r;
        break;
      }
    }

    if (found) {
      // Remove reaction
      ctx.db.messageReaction.id.delete(found.id);
    } else {
      // Add reaction
      ctx.db.messageReaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

// Create or open a direct message conversation with another user
export const createDm = spacetimedb.reducer(
  { targetIdentity: t.identity() },
  (ctx, { targetIdentity }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Set your name first');
    if (!ctx.db.user.identity.find(targetIdentity)) throw new SenderError('Target user not found');
    if (ctx.sender.toHexString() === targetIdentity.toHexString()) throw new SenderError('Cannot DM yourself');

    // Deterministic room name — always sorted alphabetically so both users get the same name
    const a = ctx.sender.toHexString();
    const b = targetIdentity.toHexString();
    const [first, second] = a < b ? [a, b] : [b, a];
    const dmName = `__dm__${first}_${second}`;

    // Check if DM room already exists
    const existing = ctx.db.room.name.find(dmName);
    if (existing) {
      // Ensure caller is a member (e.g., re-opening after leave)
      let isMember = false;
      for (const m of [...ctx.db.roomMember.roomId.filter(existing.id)]) {
        if (m.userIdentity.toHexString() === ctx.sender.toHexString()) { isMember = true; break; }
      }
      if (!isMember) {
        ctx.db.roomMember.insert({ id: 0n, roomId: existing.id, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
      }
      return;
    }

    // Create the DM room
    const roomId = ctx.db.room.insert({
      id: 0n,
      name: dmName,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
      isPrivate: true,
      isDm: true,
    }).id;

    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: targetIdentity, joinedAt: ctx.timestamp });
  }
);

// Invite a user to a private room (admin only)
export const inviteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    const room = ctx.db.room.id.find(roomId);
    if (!room) throw new SenderError('Room not found');
    if (!room.isPrivate) throw new SenderError('Only private rooms support invitations');

    // Must be admin
    let callerIsAdmin = false;
    for (const a of [...ctx.db.roomAdmin.roomId.filter(roomId)]) {
      if (a.userIdentity.toHexString() === ctx.sender.toHexString()) { callerIsAdmin = true; break; }
    }
    if (!callerIsAdmin) {
      if (room.createdBy.toHexString() === ctx.sender.toHexString()) callerIsAdmin = true;
    }
    if (!callerIsAdmin) throw new SenderError('Not authorized');

    if (!ctx.db.user.identity.find(targetIdentity)) throw new SenderError('User not found');

    // Check not already a member
    for (const m of [...ctx.db.roomMember.roomId.filter(roomId)]) {
      if (m.userIdentity.toHexString() === targetIdentity.toHexString()) {
        throw new SenderError('User is already a member');
      }
    }

    // Check not already invited
    for (const inv of [...ctx.db.roomInvitation.inviteeIdentity.filter(targetIdentity)]) {
      if (inv.roomId === roomId) throw new SenderError('User already has a pending invitation');
    }

    ctx.db.roomInvitation.insert({ id: 0n, roomId, inviterIdentity: ctx.sender, inviteeIdentity: targetIdentity, createdAt: ctx.timestamp });
  }
);

// Accept a room invitation — adds caller to the room
export const acceptInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (inv.inviteeIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Not your invitation');
    }

    const room = ctx.db.room.id.find(inv.roomId);
    if (!room) throw new SenderError('Room no longer exists');

    ctx.db.roomMember.insert({ id: 0n, roomId: inv.roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
    ctx.db.roomInvitation.id.delete(invitationId);
  }
);

// Decline a room invitation
export const declineInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (inv.inviteeIdentity.toHexString() !== ctx.sender.toHexString()) {
      throw new SenderError('Not your invitation');
    }
    ctx.db.roomInvitation.id.delete(invitationId);
  }
);

// Save or clear a message draft for a room
// If text is empty, the draft is deleted; otherwise it is upserted
export const saveDraft = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    if (!ctx.db.room.id.find(roomId)) return;

    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; text: string; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const d of [...ctx.db.draft.roomId.filter(roomId)]) {
      if (d.userIdentity.toHexString() === ctx.sender.toHexString()) {
        found = d;
        break;
      }
    }

    if (text.length === 0) {
      if (found) ctx.db.draft.id.delete(found.id);
    } else {
      if (found) {
        ctx.db.draft.id.update({ ...found, text, updatedAt: ctx.timestamp });
      } else {
        ctx.db.draft.insert({ id: 0n, roomId, userIdentity: ctx.sender, text, updatedAt: ctx.timestamp });
      }
    }
  }
);
