import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

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

const readReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
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

// Scheduled table — fires sendScheduledMessage reducer when scheduledAt time arrives
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

// Scheduled table — fires deleteEphemeralMessage reducer when expiresAt arrives
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

export const spacetimedb = schema({
  user,
  room,
  roomMember,
  message,
  messageEditHistory,
  typingIndicator,
  readReceipt,
  messageReaction,
  roomBan,
  scheduledMessage,
  ephemeralExpiry,
});

export default spacetimedb;

// Scheduled reducer: fires when scheduled message time arrives
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Verify sender is still a member of the room
    const members = [...ctx.db.roomMember.by_room.filter(timer.roomId)];
    const isMember = members.some(m => m.userId.equals(timer.sender));
    if (!isMember) return;
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

// Scheduled reducer: fires when ephemeral message should be deleted
export const deleteEphemeralMessage = spacetimedb.reducer(
  { timer: ephemeralExpiry.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
    // Clean up read receipts for this message
    const receipts = [...ctx.db.readReceipt.by_message.filter(timer.messageId)];
    for (const receipt of receipts) {
      ctx.db.readReceipt.id.delete(receipt.id);
    }
  }
);

// Export ScheduleAt so index.ts can use it
export { ScheduleAt };
