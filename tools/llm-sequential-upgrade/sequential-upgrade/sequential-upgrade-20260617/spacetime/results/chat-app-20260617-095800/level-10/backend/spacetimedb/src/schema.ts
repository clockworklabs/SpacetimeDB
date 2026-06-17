import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// Users identified by their SpacetimeDB identity
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(),          // 'online' | 'away' | 'dnd' | 'invisible'
    lastActiveAt: t.timestamp(),
  }
);

// Chat rooms
const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().unique(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
    isPrivate: t.bool(),  // private/invite-only room
    isDm: t.bool(),       // direct message room between two users
  }
);

// Room memberships (who has joined which room)
const membership = table(
  {
    name: 'membership',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_user', algorithm: 'btree', columns: ['userIdentity'] },
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
  }
);

// Chat messages
const message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_parent', algorithm: 'btree', columns: ['parentMessageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    expiresAt: t.option(t.u64()),  // Unix microseconds; null = permanent
    editedAt: t.option(t.timestamp()),  // null = never edited
    parentMessageId: t.option(t.u64()),  // null = top-level; set = thread reply
  }
);

// Typing indicators — one row per (user, room), upserted/deleted as user types
const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
    updatedAt: t.timestamp(),
  }
);

// Read receipts — last message ID each user has seen per room
const readReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
    lastReadMessageId: t.u64(),
  }
);

// Scheduled messages — one-shot timer rows; SpacetimeDB calls sendScheduledMessage at scheduled_at time
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
    indexes: [{ accessor: 'by_sender', algorithm: 'btree', columns: ['senderIdentity'] }],
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    text: t.string(),
  }
);

// Ephemeral message expiry timer — fires deleteExpiredMessage when time is up
const messageExpiry = table(
  {
    name: 'message_expiry',
    scheduled: (): any => deleteExpiredMessage,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// Edit history — one row per edit, storing the previous text
const messageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [{ accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    previousText: t.string(),
    editedAt: t.timestamp(),
  }
);

// Message reactions — one row per (user, message, emoji) pair
const messageReaction = table(
  {
    name: 'message_reaction',
    public: true,
    indexes: [
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
      { accessor: 'by_message_user', algorithm: 'btree', columns: ['messageId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

// Room admins — room creator and any promoted users
const roomAdmin = table(
  {
    name: 'room_admin',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
  }
);

// Room bans — users kicked from rooms who cannot rejoin
const roomBan = table(
  {
    name: 'room_ban',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
  }
);

// Room invitations — pending/accepted/declined invites to private rooms
const invitation = table(
  {
    name: 'invitation',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_invitee', algorithm: 'btree', columns: ['inviteeIdentity'] },
      { accessor: 'by_room_invitee', algorithm: 'btree', columns: ['roomId', 'inviteeIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    inviterIdentity: t.identity(),
    inviteeIdentity: t.identity(),
    status: t.string(),  // 'pending' | 'accepted' | 'declined'
    createdAt: t.timestamp(),
  }
);

// Global presence timer — fires every 60 seconds to auto-set idle users to 'away'
const presenceTimer = table(
  {
    name: 'presence_timer',
    scheduled: (): any => checkPresence,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
  }
);

const spacetimedb = schema({ user, room, membership, message, typingIndicator, readReceipt, scheduledMessage, messageExpiry, messageReaction, messageEdit, roomAdmin, roomBan, presenceTimer, invitation });
export default spacetimedb;
export { ScheduleAt };

// sendScheduledMessage must be defined in this file so the (): any => lambda above can close over it
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Timer row is auto-deleted after this reducer runs
    const room = ctx.db.room.id.find(timer.roomId);
    if (!room) return;
    const memberships = [...ctx.db.membership.by_room_user.filter([timer.roomId, timer.senderIdentity])];
    if (memberships.length === 0) return;
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      senderIdentity: timer.senderIdentity,
      text: timer.text,
      sentAt: ctx.timestamp,
      expiresAt: undefined,
      editedAt: undefined,
      parentMessageId: undefined,
    });
  }
);

// deleteExpiredMessage fires when a messageExpiry timer row fires
export const deleteExpiredMessage = spacetimedb.reducer(
  { timer: messageExpiry.rowType },
  (ctx, { timer }) => {
    // Timer row is auto-deleted after this reducer runs
    ctx.db.message.id.delete(timer.messageId);
  }
);

// checkPresence fires every 60 seconds; auto-sets idle 'online' users to 'away'
export const checkPresence = spacetimedb.reducer(
  { timer: presenceTimer.rowType },
  (ctx, _args) => {
    const awayThresholdMicros = 5n * 60n * 1_000_000n; // 5 minutes
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const u of [...ctx.db.user.iter()]) {
      if (!u.online || u.status !== 'online') continue;
      if (now - u.lastActiveAt.microsSinceUnixEpoch > awayThresholdMicros) {
        ctx.db.user.identity.update({ ...u, status: 'away' });
      }
    }
  }
);
