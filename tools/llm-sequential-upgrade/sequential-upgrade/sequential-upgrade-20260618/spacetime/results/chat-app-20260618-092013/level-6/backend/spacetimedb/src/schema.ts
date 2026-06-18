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
    isAdmin: t.bool(),
  }
);

const roomBan = table(
  {
    name: 'room_ban',
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
    expiresAt: t.option(t.u64()), // micros since Unix epoch; undefined = permanent
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
    sendAt: t.u64(),
  }
);

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

const ephemeralMessageTimer = table(
  {
    name: 'ephemeral_message_timer',
    scheduled: (): any => deleteExpiredMessages,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
  }
);

const messageReaction = table(
  {
    name: 'message_reaction',
    public: true,
    indexes: [
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

const messageEditHistory = table(
  {
    name: 'message_edit_history',
    public: true,
    indexes: [
      { accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    previousContent: t.string(),
    editedAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  user, room, roomMember, roomBan, message, typingIndicator, userRoomRead,
  scheduledMessage, scheduledMessageTimer, ephemeralMessageTimer, messageReaction,
  messageEditHistory,
});
export default spacetimedb;
export { scheduledMessageTimer, ephemeralMessageTimer };

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
          expiresAt: undefined,
        });
        ctx.db.scheduledMessage.id.delete(pending.id);
      }
    }
  }
);

export const deleteExpiredMessages = spacetimedb.reducer(
  { timer: ephemeralMessageTimer.rowType },
  (ctx, { timer: _timer }) => {
    const now = ctx.timestamp.microsSinceUnixEpoch;
    for (const msg of [...ctx.db.message.iter()]) {
      if (msg.expiresAt !== undefined && msg.expiresAt <= now) {
        ctx.db.message.id.delete(msg.id);
      }
    }
  }
);
