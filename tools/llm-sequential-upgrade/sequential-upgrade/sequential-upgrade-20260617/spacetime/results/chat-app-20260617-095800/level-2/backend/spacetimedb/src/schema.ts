import { schema, table, t } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';

// Users identified by their SpacetimeDB identity
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
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
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
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

const spacetimedb = schema({ user, room, membership, message, typingIndicator, readReceipt, scheduledMessage });
export default spacetimedb;
export { ScheduleAt };

// sendScheduledMessage must be defined in this file so the (): any => lambda above can close over it
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    // Timer row is auto-deleted after this reducer runs
    if (!ctx.db.room.id.find(timer.roomId)) return;
    const memberships = [...ctx.db.membership.by_room_user.filter([timer.roomId, timer.senderIdentity])];
    if (memberships.length === 0) return;
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      senderIdentity: timer.senderIdentity,
      text: timer.text,
      sentAt: ctx.timestamp,
    });
  }
);
