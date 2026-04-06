import { schema, table, t } from 'spacetimedb/server';

// User profile table
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    createdAt: t.timestamp(),
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

// Room membership
const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    joinedAt: t.timestamp(),
  }
);

// Messages
const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    senderIdentity: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    expiresAt: t.option(t.timestamp()),
  }
);

// Typing indicators
const typingIndicator = table(
  { name: 'typing_indicator', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    updatedAt: t.timestamp(),
  }
);

// Read receipts — track last read message per user per room
const readReceipt = table(
  { name: 'read_receipt', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity().index('btree'),
    lastReadMessageId: t.u64(),
    updatedAt: t.timestamp(),
  }
);

// Pending scheduled messages (visible to author)
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    roomId: t.u64().index('btree'),
    senderIdentity: t.identity().index('btree'),
    text: t.string(),
  }
);

// Message expiry — auto-deletes ephemeral messages when time expires
const messageExpiry = table(
  {
    name: 'message_expiry',
    scheduled: (): any => deleteExpiredMessage,
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  }
);

const spacetimedb = schema({ user, room, roomMember, message, typingIndicator, readReceipt, scheduledMessage, messageExpiry });
export default spacetimedb;

// Fires when a scheduled message is due — inserts it as a real message
export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    const msg = ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      senderIdentity: timer.senderIdentity,
      text: timer.text,
      sentAt: ctx.timestamp,
      expiresAt: null,
    });

    // Update read receipt for the original sender
    let found: { id: bigint; roomId: bigint; userIdentity: { toHexString(): string }; lastReadMessageId: bigint; updatedAt: { microsSinceUnixEpoch: bigint } } | undefined;
    for (const r of [...ctx.db.readReceipt.roomId.filter(timer.roomId)]) {
      if (r.userIdentity.toHexString() === timer.senderIdentity.toHexString()) {
        found = r;
        break;
      }
    }
    if (found) {
      ctx.db.readReceipt.id.update({ ...found, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    } else {
      ctx.db.readReceipt.insert({ id: 0n, roomId: timer.roomId, userIdentity: timer.senderIdentity, lastReadMessageId: msg.id, updatedAt: ctx.timestamp });
    }
  }
);

// Fires when an ephemeral message expires — permanently deletes it
export const deleteExpiredMessage = spacetimedb.reducer(
  { timer: messageExpiry.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
  }
);
