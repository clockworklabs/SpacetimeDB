import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// Forward declarations for scheduled reducer references
let deliverScheduledMessage: any;
let deleteEphemeralMessage: any;

// ── Tables ─────────────────────────────────────────────────────────────────

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(), // 'online' | 'away' | 'dnd' | 'invisible'
    lastActive: t.timestamp(),
  }
);

const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
  }
);

const roomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    isAdmin: t.bool(),
    isBanned: t.bool(),
  }
);

const message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    edited: t.bool(),
    ephemeralExpiry: t.option(t.timestamp()),
  }
);

const messageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    oldText: t.string(),
    editedAt: t.timestamp(),
  }
);

const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    expiresAt: t.timestamp(),
  }
);

const readReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    lastReadMessageId: t.u64(),
  }
);

const reaction = table(
  {
    name: 'reaction',
    public: true,
    indexes: [
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    identity: t.identity(),
    emoji: t.string(),
  }
);

const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    indexes: [
      { accessor: 'by_author', algorithm: 'btree', columns: ['author'] },
    ],
    scheduled: (): any => deliverScheduledMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    author: t.identity(),
    text: t.string(),
  }
);

const ephemeralExpiry = table(
  {
    name: 'ephemeral_expiry',
    public: false,
    scheduled: (): any => deleteEphemeralMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ── Schema ─────────────────────────────────────────────────────────────────

const spacetimedb = schema({
  user,
  room,
  roomMember,
  message,
  messageEdit,
  typingIndicator,
  readReceipt,
  reaction,
  scheduledMessage,
  ephemeralExpiry,
});

export default spacetimedb;

// ── Lifecycle ──────────────────────────────────────────────────────────────

export const onConnect = spacetimedb.clientConnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: true, status: 'online', lastActive: ctx.timestamp });
  }
});

export const onDisconnect = spacetimedb.clientDisconnected((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) {
    ctx.db.user.identity.update({ ...existing, online: false, lastActive: ctx.timestamp });
  }
});

// ── User Management ────────────────────────────────────────────────────────

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 32) throw new SenderError('Name must be 1-32 chars');
    if (ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Already registered');
    ctx.db.user.insert({ identity: ctx.sender, name: trimmed, online: true, status: 'online', lastActive: ctx.timestamp });
  }
);

export const setStatus = spacetimedb.reducer(
  { status: t.string() },
  (ctx, { status }) => {
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) throw new SenderError('Invalid status');
    const existing = ctx.db.user.identity.find(ctx.sender);
    if (!existing) throw new SenderError('Not registered');
    ctx.db.user.identity.update({ ...existing, status, lastActive: ctx.timestamp });
  }
);

export const updateActivity = spacetimedb.reducer((ctx) => {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (!existing) return;
  const newStatus = existing.status === 'away' ? 'online' : existing.status;
  ctx.db.user.identity.update({ ...existing, lastActive: ctx.timestamp, status: newStatus });
});

// ── Room Management ────────────────────────────────────────────────────────

export const createRoom = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmed = name.trim();
    if (trimmed.length === 0 || trimmed.length > 64) throw new SenderError('Room name must be 1-64 chars');
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    const roomRow = ctx.db.room.insert({ id: 0n, name: trimmed, createdBy: ctx.sender });
    ctx.db.roomMember.insert({ id: 0n, roomId: roomRow.id, identity: ctx.sender, isAdmin: true, isBanned: false });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new SenderError('Not registered');
    if (!ctx.db.room.id.find(roomId)) throw new SenderError('Room not found');
    for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
      if (m.identity.equals(ctx.sender)) {
        if (m.isBanned) throw new SenderError('You are banned from this room');
        return;
      }
    }
    ctx.db.roomMember.insert({ id: 0n, roomId, identity: ctx.sender, isAdmin: false, isBanned: false });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
      if (m.identity.equals(ctx.sender)) {
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
  }
);

// ── Admin / Permissions ────────────────────────────────────────────────────

function getAdminMembership(ctx: any, roomId: bigint) {
  for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
    if (m.identity.equals(ctx.sender) && m.isAdmin) return m;
  }
  throw new SenderError('Not an admin of this room');
}

export const kickUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    getAdminMembership(ctx, roomId);
    for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        ctx.db.roomMember.id.delete(m.id);
        return;
      }
    }
  }
);

export const banUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    getAdminMembership(ctx, roomId);
    for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        ctx.db.roomMember.id.update({ ...m, isBanned: true });
        return;
      }
    }
    ctx.db.roomMember.insert({ id: 0n, roomId, identity: targetIdentity, isAdmin: false, isBanned: true });
  }
);

export const promoteUser = spacetimedb.reducer(
  { roomId: t.u64(), targetIdentity: t.identity() },
  (ctx, { roomId, targetIdentity }) => {
    getAdminMembership(ctx, roomId);
    for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
      if (m.identity.equals(targetIdentity)) {
        ctx.db.roomMember.id.update({ ...m, isAdmin: true });
        return;
      }
    }
    throw new SenderError('User not in this room');
  }
);

// ── Messages ───────────────────────────────────────────────────────────────

function requireMembership(ctx: any, roomId: bigint) {
  for (const m of ctx.db.roomMember.by_room.filter(roomId)) {
    if (m.identity.equals(ctx.sender)) {
      if (m.isBanned) throw new SenderError('You are banned from this room');
      return m;
    }
  }
  throw new SenderError('Not a member of this room');
}

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    requireMembership(ctx, roomId);
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 chars');
    ctx.db.message.insert({ id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp, edited: false, ephemeralExpiry: undefined });
  }
);

export const sendEphemeralMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), ttlSeconds: t.u32() },
  (ctx, { roomId, text, ttlSeconds }) => {
    requireMembership(ctx, roomId);
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 chars');
    if (ttlSeconds < 10 || ttlSeconds > 3600) throw new SenderError('TTL must be 10-3600 seconds');
    const ttlMicros = BigInt(ttlSeconds) * 1_000_000n;
    const expiryMicros = ctx.timestamp.microsSinceUnixEpoch + ttlMicros;
    const msg = ctx.db.message.insert({
      id: 0n, roomId, sender: ctx.sender, text: trimmed, sentAt: ctx.timestamp, edited: false,
      ephemeralExpiry: { microsSinceUnixEpoch: expiryMicros },
    });
    ctx.db.ephemeralExpiry.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiryMicros),
      messageId: msg.id,
    });
  }
);

deleteEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralExpiry.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
  }
);
export const _deleteEphemeralMessage = deleteEphemeralMessage;

export const editMessage = spacetimedb.reducer(
  { messageId: t.u64(), newText: t.string() },
  (ctx, { messageId, newText }) => {
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!msg.sender.equals(ctx.sender)) throw new SenderError('Not your message');
    const trimmed = newText.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 chars');
    ctx.db.messageEdit.insert({ id: 0n, messageId, oldText: msg.text, editedAt: ctx.timestamp });
    ctx.db.message.id.update({ ...msg, text: trimmed, edited: true });
  }
);

// ── Typing Indicators ──────────────────────────────────────────────────────

export const setTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    requireMembership(ctx, roomId);
    const expiryMicros = ctx.timestamp.microsSinceUnixEpoch + 5_000_000n;
    const expiryTs = { microsSinceUnixEpoch: expiryMicros };
    for (const ti of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (ti.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.update({ ...ti, expiresAt: expiryTs });
        return;
      }
    }
    ctx.db.typingIndicator.insert({ id: 0n, roomId, identity: ctx.sender, expiresAt: expiryTs });
  }
);

export const clearTyping = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    for (const ti of ctx.db.typingIndicator.by_room.filter(roomId)) {
      if (ti.identity.equals(ctx.sender)) {
        ctx.db.typingIndicator.id.delete(ti.id);
        return;
      }
    }
  }
);

// ── Read Receipts ──────────────────────────────────────────────────────────

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), messageId: t.u64() },
  (ctx, { roomId, messageId }) => {
    requireMembership(ctx, roomId);
    for (const rr of ctx.db.readReceipt.by_room.filter(roomId)) {
      if (rr.identity.equals(ctx.sender)) {
        if (messageId > rr.lastReadMessageId) {
          ctx.db.readReceipt.id.update({ ...rr, lastReadMessageId: messageId });
        }
        return;
      }
    }
    ctx.db.readReceipt.insert({ id: 0n, roomId, identity: ctx.sender, lastReadMessageId: messageId });
  }
);

// ── Reactions ──────────────────────────────────────────────────────────────

const VALID_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

export const toggleReaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, { messageId, emoji }) => {
    if (!VALID_EMOJIS.includes(emoji)) throw new SenderError('Invalid emoji');
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    requireMembership(ctx, msg.roomId);
    for (const r of ctx.db.reaction.by_message.filter(messageId)) {
      if (r.identity.equals(ctx.sender) && r.emoji === emoji) {
        ctx.db.reaction.id.delete(r.id);
        return;
      }
    }
    ctx.db.reaction.insert({ id: 0n, messageId, identity: ctx.sender, emoji });
  }
);

// ── Scheduled Messages ─────────────────────────────────────────────────────

export const scheduleMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string(), sendAtMicros: t.u64() },
  (ctx, { roomId, text, sendAtMicros }) => {
    requireMembership(ctx, roomId);
    const trimmed = text.trim();
    if (trimmed.length === 0 || trimmed.length > 2000) throw new SenderError('Message must be 1-2000 chars');
    if (sendAtMicros <= ctx.timestamp.microsSinceUnixEpoch) throw new SenderError('Must schedule in the future');
    ctx.db.scheduledMessage.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(sendAtMicros),
      roomId,
      author: ctx.sender,
      text: trimmed,
    });
  }
);

export const cancelScheduledMessage = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    const sm = ctx.db.scheduledMessage.scheduledId.find(scheduledId);
    if (!sm) throw new SenderError('Not found');
    if (!sm.author.equals(ctx.sender)) throw new SenderError('Not your message');
    ctx.db.scheduledMessage.scheduledId.delete(scheduledId);
  }
);

deliverScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    ctx.db.message.insert({
      id: 0n, roomId: timer.roomId, sender: timer.author, text: timer.text,
      sentAt: ctx.timestamp, edited: false, ephemeralExpiry: undefined,
    });
  }
);
export const _deliverScheduledMessage = deliverScheduledMessage;
