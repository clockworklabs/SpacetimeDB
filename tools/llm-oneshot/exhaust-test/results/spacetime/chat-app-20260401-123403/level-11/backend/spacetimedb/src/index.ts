import spacetimedb from './schema';
import { t, SenderError } from 'spacetimedb/server';
export { default, init, cleanupTyping, sendScheduledMessage } from './schema';
// threadReply table exported as part of schema default
// roomPermission table is exported as part of the schema default export
import { ScheduleAt } from 'spacetimedb';

// ── Lifecycle hooks ────────────────────────────────────────────────────────

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    // Restore to online unless user chose invisible/dnd
    const newStatus = existing.status === 'dnd' || existing.status === 'invisible'
      ? existing.status
      : 'online';
    ctx.db.user.identity.update({ ...existing, online: true, status: newStatus, lastActiveAt: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActiveAt: ctx.timestamp });
  }
  // Clear typing states for this user
  for (const row of [...ctx.db.typingState.iter()]) {
    if (row.userIdentity.equals(ctx.sender)) {
      ctx.db.typingState.id.delete(row.id);
    }
  }
});

// ── User reducers ──────────────────────────────────────────────────────────

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 characters)');
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Already registered');
    ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true, status: 'online', lastActiveAt: ctx.timestamp });
  }
);

export const updateName = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Name cannot be empty');
    if (trimmed.length > 32) throw new SenderError('Name too long (max 32 characters)');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, name: trimmed });
  }
);

// ── Room reducers ──────────────────────────────────────────────────────────

export const createRoom = spacetimedb.reducer(
  { name: t.string(), isPrivate: t.bool() },
  (ctx, { name, isPrivate }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0) throw new SenderError('Room name cannot be empty');
    if (trimmed.length > 64) throw new SenderError('Room name too long (max 64 characters)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    // Insert room
    ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate, isDm: false });
    // Auto-join the creator: find the room by scanning for latest by this user
    let maxId = 0n;
    for (const r of ctx.db.room.iter()) {
      if (r.createdBy.equals(ctx.sender) && r.name === trimmed && r.id > maxId) {
        maxId = r.id;
      }
    }
    if (maxId > 0n) {
      ctx.db.roomMember.insert({ id: 0n, roomId: maxId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
      // Creator becomes admin
      ctx.db.roomPermission.insert({ id: 0n, roomId: maxId, userIdentity: ctx.sender, role: 'admin' });
    }
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const roomRow = ctx.db.room.id.find(roomId);
    if (!roomRow) throw new SenderError('Room not found');
    // Private rooms require an accepted invitation
    if (roomRow.isPrivate) {
      const hasInvitation = [...ctx.db.roomInvitation.roomId.filter(roomId)]
        .some(inv => inv.inviteeIdentity.equals(ctx.sender) && inv.status === 'accepted');
      if (!hasInvitation) throw new SenderError('This is a private room. You need an invitation.');
    }
    // Check not banned
    const isBanned = [...ctx.db.roomPermission.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(ctx.sender) && row.role === 'banned');
    if (isBanned) throw new SenderError('You are banned from this room');
    // Check not already a member
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(ctx.sender));
    if (alreadyMember) throw new SenderError('Already a member');
    ctx.db.roomMember.insert({ id: 0n, roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const membership = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find(row => row.userIdentity.equals(ctx.sender));
    if (!membership) throw new SenderError('Not a member of this room');
    ctx.db.roomMember.id.delete(membership.id);
    // Clean up typing state
    const typing = [...ctx.db.typingState.roomId.filter(roomId)]
      .find(row => row.userIdentity.equals(ctx.sender));
    if (typing) ctx.db.typingState.id.delete(typing.id);
    // Clean up user room state
    const state = [...ctx.db.userRoomState.userIdentity.filter(ctx.sender)]
      .find(row => row.roomId === roomId);
    if (state) ctx.db.userRoomState.id.delete(state.id);
  }
);

// ── Message reducers ───────────────────────────────────────────────────────

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), ttlSecs: t.u64() },
  (ctx, { roomId, text, ttlSecs }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 1000) throw new SenderError('Message too long (max 1000 characters)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(ctx.sender));
    if (!isMember) throw new SenderError('Not a member of this room');
    // Check not banned
    const isBanned = [...ctx.db.roomPermission.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(ctx.sender) && row.role === 'banned');
    if (isBanned) throw new SenderError('You are banned from this room');
    // Compute expiry: 0 means never, otherwise now + ttlSecs
    const expiresAtMicros = ttlSecs > 0n
      ? ctx.timestamp.microsSinceUnixEpoch + ttlSecs * 1_000_000n
      : 0n;
    // Insert message
    ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp, expiresAtMicros });
    // Find the inserted message id (last in room with this sender and text at this timestamp)
    let msgId = 0n;
    for (const m of ctx.db.message.roomId.filter(roomId)) {
      if (m.sender.equals(ctx.sender) && m.text === trimmed && m.id > msgId) {
        msgId = m.id;
      }
    }
    // Auto-mark as read for sender
    if (msgId > 0n) {
      const existing = [...ctx.db.userRoomState.userIdentity.filter(ctx.sender)]
        .find(row => row.roomId === roomId);
      if (existing) {
        ctx.db.userRoomState.id.update({ ...existing, lastReadMessageId: msgId });
      } else {
        ctx.db.userRoomState.insert({ id: 0n, userIdentity: ctx.sender, roomId, lastReadMessageId: msgId });
      }
    }
  }
);

// ── Typing indicators ──────────────────────────────────────────────────────

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64(), isTyping: t.bool() },
  (ctx, { roomId, isTyping }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    // Remove existing typing state for this user in this room
    const existing = [...ctx.db.typingState.roomId.filter(roomId)]
      .find(row => row.userIdentity.equals(ctx.sender));
    if (existing) ctx.db.typingState.id.delete(existing.id);
    if (isTyping) {
      // Set new typing state expiring in 4 seconds
      ctx.db.typingState.insert({
        id: 0n,
        roomId,
        userIdentity: ctx.sender,
        expiresAtMicros: ctx.timestamp.microsSinceUnixEpoch + 4_000_000n,
      });
    }
  }
);

// ── Read receipts ──────────────────────────────────────────────────────────

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    const existing = [...ctx.db.userRoomState.userIdentity.filter(ctx.sender)]
      .find(row => row.roomId === roomId);
    if (existing) {
      if (messageId > existing.lastReadMessageId) {
        ctx.db.userRoomState.id.update({ ...existing, lastReadMessageId: messageId });
      }
    } else {
      ctx.db.userRoomState.insert({ id: 0n, userIdentity: ctx.sender, roomId, lastReadMessageId: messageId });
    }
  }
);

// ── Scheduled messages ─────────────────────────────────────────────────────

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 1000) throw new SenderError('Message too long (max 1000 characters)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(ctx.sender));
    if (!isMember) throw new SenderError('Not a member of this room');
    const now = ctx.timestamp.microsSinceUnixEpoch;
    if (sendAtMicros <= now) throw new SenderError('Scheduled time must be in the future');
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
      roomId,
      sender: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const row = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!row) throw new SenderError('Scheduled message not found');
    if (!row.sender.equals(ctx.sender)) throw new SenderError('Not your scheduled message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

// ── Reactions ──────────────────────────────────────────────────────────────

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.message.id.find(messageId)) throw new SenderError('Message not found');
    // Toggle: remove if exists, add if not
    const existing = [...ctx.db.reaction.messageId.filter(messageId)]
      .find(r => r.userIdentity.equals(ctx.sender) && r.emoji === emoji);
    if (existing) {
      ctx.db.reaction.id.delete(existing.id);
    } else {
      ctx.db.reaction.insert({ id: 0n, messageId, userIdentity: ctx.sender, emoji });
    }
  }
);

// ── Room permissions ───────────────────────────────────────────────────────

function isAdmin(ctx: any, roomId: bigint, identity: any): boolean {
  return [...ctx.db.roomPermission.roomId.filter(roomId)]
    .some((row: any) => row.userIdentity.equals(identity) && row.role === 'admin');
}

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    if (!isAdmin(ctx, roomId, ctx.sender)) throw new SenderError('Not an admin of this room');
    if (targetIdentity.equals(ctx.sender)) throw new SenderError('Cannot kick yourself');
    // Remove from room membership
    const membership = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (!membership) throw new SenderError('User is not a member');
    ctx.db.roomMember.id.delete(membership.id);
    // Clean up typing state
    const typing = [...ctx.db.typingState.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (typing) ctx.db.typingState.id.delete(typing.id);
    // Clean up user room state
    const state = [...ctx.db.userRoomState.userIdentity.filter(targetIdentity)]
      .find((row: any) => row.roomId === roomId);
    if (state) ctx.db.userRoomState.id.delete(state.id);
  }
);

export const banUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    if (!isAdmin(ctx, roomId, ctx.sender)) throw new SenderError('Not an admin of this room');
    if (targetIdentity.equals(ctx.sender)) throw new SenderError('Cannot ban yourself');
    // Remove from membership if present
    const membership = [...ctx.db.roomMember.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (membership) ctx.db.roomMember.id.delete(membership.id);
    // Remove any existing permission entry for this user in this room
    const existing = [...ctx.db.roomPermission.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (existing) ctx.db.roomPermission.id.delete(existing.id);
    // Add ban
    ctx.db.roomPermission.insert({ id: 0n, roomId, userIdentity: targetIdentity, role: 'banned' });
    // Clean up typing state
    const typing = [...ctx.db.typingState.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (typing) ctx.db.typingState.id.delete(typing.id);
    // Clean up user room state
    const state = [...ctx.db.userRoomState.userIdentity.filter(targetIdentity)]
      .find((row: any) => row.roomId === roomId);
    if (state) ctx.db.userRoomState.id.delete(state.id);
  }
);

export const promoteAdmin = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    if (!isAdmin(ctx, roomId, ctx.sender)) throw new SenderError('Not an admin of this room');
    // Target must be a member
    const isMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some((row: any) => row.userIdentity.equals(targetIdentity));
    if (!isMember) throw new SenderError('User is not a member');
    // Already admin?
    const already = [...ctx.db.roomPermission.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity) && row.role === 'admin');
    if (already) return; // Already admin, no-op
    // Remove any other permission entry (e.g., if somehow had banned status but was re-added)
    const existing = [...ctx.db.roomPermission.roomId.filter(roomId)]
      .find((row: any) => row.userIdentity.equals(targetIdentity));
    if (existing) ctx.db.roomPermission.id.delete(existing.id);
    // Grant admin
    ctx.db.roomPermission.insert({ id: 0n, roomId, userIdentity: targetIdentity, role: 'admin' });
  }
);

// ── Presence / status ──────────────────────────────────────────────────────

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, status, lastActiveAt: ctx.timestamp });
  }
);

// ── Message editing ────────────────────────────────────────────────────────

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const trimmed = newText.trim();
    if (trimmed.length === 0) throw new SenderError('Message cannot be empty');
    if (trimmed.length > 1000) throw new SenderError('Message too long (max 1000 characters)');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Can only edit your own messages');
    if (msg.text === trimmed) return; // No change
    // Record the edit in history
    ctx.db.messageEdit.insert({
      id: 0n,
      messageId,
      editedAt: ctx.timestamp,
      oldText: msg.text,
      newText: trimmed,
    });
    // Update the message text
    ctx.db.message.id.update({ ...msg, text: trimmed });
  }
);

// ── Private rooms / DMs / Invitations ─────────────────────────────────────

export const inviteToRoom = spacetimedb.reducer(
  { roomId: t.u64(), inviteeIdentity: t.identity() },
  (ctx, { roomId, inviteeIdentity }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const roomRow = ctx.db.room.id.find(roomId);
    if (!roomRow) throw new SenderError('Room not found');
    if (!roomRow.isPrivate) throw new SenderError('Room is not private');
    if (!isAdmin(ctx, roomId, ctx.sender)) throw new SenderError('Not an admin of this room');
    if (!ctx.db.user.identity.find(inviteeIdentity)) throw new SenderError('User not found');
    if (inviteeIdentity.equals(ctx.sender)) throw new SenderError('Cannot invite yourself');
    // Check not already a member
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(roomId)]
      .some(row => row.userIdentity.equals(inviteeIdentity));
    if (alreadyMember) throw new SenderError('User is already a member');
    // Check no pending invitation
    const existing = [...ctx.db.roomInvitation.roomId.filter(roomId)]
      .find(inv => inv.inviteeIdentity.equals(inviteeIdentity) && inv.status === 'pending');
    if (existing) throw new SenderError('Invitation already sent');
    ctx.db.roomInvitation.insert({
      id: 0n,
      roomId,
      inviterIdentity: ctx.sender,
      inviteeIdentity,
      sentAt: ctx.timestamp,
      status: 'pending',
    });
  }
);

export const acceptInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (!inv.inviteeIdentity.equals(ctx.sender)) throw new SenderError('Not your invitation');
    if (inv.status !== 'pending') throw new SenderError('Invitation already resolved');
    ctx.db.roomInvitation.id.update({ ...inv, status: 'accepted' });
    // Add as member if not already
    const alreadyMember = [...ctx.db.roomMember.roomId.filter(inv.roomId)]
      .some(row => row.userIdentity.equals(ctx.sender));
    if (!alreadyMember) {
      ctx.db.roomMember.insert({ id: 0n, roomId: inv.roomId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
    }
  }
);

export const declineInvitation = spacetimedb.reducer(
  { invitationId: t.u64() },
  (ctx, { invitationId }) => {
    const inv = ctx.db.roomInvitation.id.find(invitationId);
    if (!inv) throw new SenderError('Invitation not found');
    if (!inv.inviteeIdentity.equals(ctx.sender)) throw new SenderError('Not your invitation');
    if (inv.status !== 'pending') throw new SenderError('Invitation already resolved');
    ctx.db.roomInvitation.id.update({ ...inv, status: 'declined' });
  }
);

export const openDm = spacetimedb.reducer(
  { targetIdentity: t.identity() },
  (ctx, { targetIdentity }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.user.identity.find(targetIdentity)) throw new SenderError('Target user not found');
    if (targetIdentity.equals(ctx.sender)) throw new SenderError('Cannot DM yourself');
    // Check if a DM room already exists between these two users
    for (const r of [...ctx.db.room.iter()]) {
      if (!r.isDm) continue;
      const members = [...ctx.db.roomMember.roomId.filter(r.id)];
      if (members.length === 2 &&
          members.some(m => m.userIdentity.equals(ctx.sender)) &&
          members.some(m => m.userIdentity.equals(targetIdentity))) {
        // DM already exists — ensure sender is a member (they might have left)
        const isMember = members.some(m => m.userIdentity.equals(ctx.sender));
        if (!isMember) {
          ctx.db.roomMember.insert({ id: 0n, roomId: r.id, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
        }
        return; // DM already exists
      }
    }
    // Create a new DM room
    const senderUser = ctx.db.user.identity.find(ctx.sender)!;
    const targetUser = ctx.db.user.identity.find(targetIdentity)!;
    const dmName = [senderUser.name, targetUser.name].sort().join(' & ');
    ctx.db.room.insert({ id: 0n, name: dmName, createdBy: ctx.sender, createdAt: ctx.timestamp, isPrivate: true, isDm: true });
    // Find the new room id
    let maxId = 0n;
    for (const r of ctx.db.room.iter()) {
      if (r.isDm && r.createdBy.equals(ctx.sender) && r.name === dmName && r.id > maxId) {
        maxId = r.id;
      }
    }
    if (maxId > 0n) {
      ctx.db.roomMember.insert({ id: 0n, roomId: maxId, userIdentity: ctx.sender, joinedAt: ctx.timestamp });
      ctx.db.roomMember.insert({ id: 0n, roomId: maxId, userIdentity: targetIdentity, joinedAt: ctx.timestamp });
      // Both are admins in DM
      ctx.db.roomPermission.insert({ id: 0n, roomId: maxId, userIdentity: ctx.sender, role: 'admin' });
      ctx.db.roomPermission.insert({ id: 0n, roomId: maxId, userIdentity: targetIdentity, role: 'admin' });
      // Auto-accept: mark invitation as accepted for both parties (no explicit invite needed)
    }
  }
);

// ── Draft sync ─────────────────────────────────────────────────────────────

export const saveDraft = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) return;
    const existing = [...ctx.db.messageDraft.userIdentity.filter(ctx.sender)]
      .find(d => d.roomId === roomId);
    if (text.length === 0) {
      if (existing) ctx.db.messageDraft.id.delete(existing.id);
    } else {
      if (existing) {
        ctx.db.messageDraft.id.update({ ...existing, text, updatedAt: ctx.timestamp });
      } else {
        ctx.db.messageDraft.insert({ id: 0n, userIdentity: ctx.sender, roomId, text, updatedAt: ctx.timestamp });
      }
    }
  }
);

// ── Message threading ──────────────────────────────────────────────────────

export const sendThreadReply = spacetimedb.reducer(
  { parentMessageId: t.u64(), text: t.string() },
  (ctx, { parentMessageId, text }) => {
    const trimmed = text.trim();
    if (trimmed.length === 0) throw new SenderError('Reply cannot be empty');
    if (trimmed.length > 1000) throw new SenderError('Reply too long (max 1000 characters)');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const parentMsg = ctx.db.message.id.find(parentMessageId);
    if (!parentMsg) throw new SenderError('Message not found');
    const isMember = [...ctx.db.roomMember.roomId.filter(parentMsg.roomId)]
      .some(row => row.userIdentity.equals(ctx.sender));
    if (!isMember) throw new SenderError('Not a member of this room');
    const isBanned = [...ctx.db.roomPermission.roomId.filter(parentMsg.roomId)]
      .some(row => row.userIdentity.equals(ctx.sender) && row.role === 'banned');
    if (isBanned) throw new SenderError('You are banned from this room');
    ctx.db.threadReply.insert({
      id: 0n,
      parentMessageId,
      roomId: parentMsg.roomId,
      sender: ctx.sender,
      text: trimmed,
      sentAt: ctx.timestamp,
    });
  }
);
