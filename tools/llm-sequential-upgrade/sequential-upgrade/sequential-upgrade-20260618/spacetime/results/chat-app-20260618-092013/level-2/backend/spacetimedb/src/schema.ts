import { schema, table, t } from 'spacetimedb/server';

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
  }
);

const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
  }
);

const roomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
  }
);

const message = table(
  {
    name: 'message',
    public: true,
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    content: t.string(),
    sentAt: t.timestamp(),
  }
);

const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    updatedAt: t.timestamp(),
  }
);

const userRoomRead = table(
  {
    name: 'user_room_read',
    public: true,
    indexes: [
      { accessor: 'by_room_user', algorithm: 'btree', columns: ['roomId', 'userIdentity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    lastReadMessageId: t.u64(),
  }
);

// Stores user-scheduled messages pending delivery (public so authors can see their own)
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    indexes: [{ accessor: 'by_sender', algorithm: 'btree', columns: ['sender'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    sender: t.identity(),
    content: t.string(),
    sendAt: t.u64(), // microseconds since Unix epoch
  }
);

// Repeating timer that polls for due scheduled messages every 10 seconds
const scheduledMessageTimer = table(
  {
    name: 'scheduled_message_timer',
    scheduled: (): any => processScheduledMessages,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
  }
);

const spacetimedb = schema({ user, room, roomMember, message, typingIndicator, userRoomRead, scheduledMessage, scheduledMessageTimer });
export default spacetimedb;
export { scheduledMessageTimer };

// Fires every 10 seconds to deliver any due scheduled messages
export const processScheduledMessages = spacetimedb.reducer(
  { timer: scheduledMessageTimer.rowType },
  (ctx, { timer: _timer }) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const pending of [...ctx.db.scheduledMessage.iter()]) {
      if (pending.sendAt <= now) {
        ctx.db.message.insert({
          id: 0n,
          roomId: pending.roomId,
          sender: pending.sender,
          content: pending.content,
          sentAt: ctx.timestamp,
        });
        ctx.db.scheduledMessage.id.delete(pending.id);
      }
    }
  }
);
