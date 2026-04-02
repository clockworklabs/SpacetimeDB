import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// ─── Tables ───────────────────────────────────────────────────────────────────

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    // 'online' | 'away' | 'dnd' | 'invisible'
    status: t.string(),
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
      { accessor: 'by_user', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    isAdmin: t.bool(),
  }
);

const roomBan = table(
  {
    name: 'room_ban',
    public: true,
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
  }
);

const message = table(
  {
    name: 'message',
    public: true,
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    editedAt: t.option(t.timestamp()),
    isEphemeral: t.bool(),
    expiresAt: t.option(t.timestamp()),
  }
);

const messageEditHistory = table(
  {
    name: 'message_edit_history',
    public: true,
    indexes: [{ accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    text: t.string(),
    editedAt: t.timestamp(),
  }
);

const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    lastTypingAt: t.timestamp(),
  }
);

// Tracks last-read message per user per room (for unread counts & read receipts)
const readReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userId: t.identity(),
    messageId: t.u64(),
  }
);

const messageReaction = table(
  {
    name: 'message_reaction',
    public: true,
    indexes: [{ accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userId: t.identity(),
    emoji: t.string(),
  }
);

// Scheduled table — fires sendScheduledMessage when scheduledAt arrives
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64(),
    sender: t.identity(),
    text: t.string(),
  }
);

// Scheduled table — fires deleteEphemeralMessage when message should expire
const ephemeralExpiry = table(
  {
    name: 'ephemeral_expiry',
    public: true,
    scheduled: (): any => deleteEphemeralMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

// ─── Schema ───────────────────────────────────────────────────────────────────

export const spacetimedb = schema({
  user,
  room,
  roomMember,
  roomBan,
  message,
  messageEditHistory,
  typingIndicator,
  readReceipt,
  messageReaction,
  scheduledMessage,
  ephemeralExpiry,
});

export default spacetimedb;

// ─── Scheduled Reducers (must live in same file as their tables) ───────────────

export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    const members = [...ctx.db.roomMember.by_room.filter(timer.roomId)];
    if (!members.some((m) => m.userId.equals(timer.sender))) return;
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      sender: timer.sender,
      text: timer.text,
      sentAt: ctx.timestamp,
      editedAt: undefined,
      isEphemeral: false,
      expiresAt: undefined,
    });
  }
);

export const deleteEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralExpiry.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
    // Remove any reactions for the deleted message
    const reactions = [...ctx.db.messageReaction.by_message.filter(timer.messageId)];
    for (const r of reactions) ctx.db.messageReaction.id.delete(r.id);
    // Remove edit history
    const history = [...ctx.db.messageEditHistory.by_message.filter(timer.messageId)];
    for (const h of history) ctx.db.messageEditHistory.id.delete(h.id);
  }
);

export { ScheduleAt };
