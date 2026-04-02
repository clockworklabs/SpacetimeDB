import { schema, table, t } from 'spacetimedb/server';

// Users table
const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(), // "online" | "away" | "dnd" | "invisible"
    lastActive: t.timestamp(),
  }
);

// Rooms table
const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().unique(),
    creatorIdentity: t.identity(),
  }
);

// Room membership
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

// Messages table
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
    editedAt: t.option(t.timestamp()),
    isEphemeral: t.bool(),
    ephemeralDurationSeconds: t.u32(),
    isDeleted: t.bool(),
  }
);

// Message edit history
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

// Message reactions
const reaction = table(
  {
    name: 'reaction',
    public: true,
    indexes: [{ accessor: 'by_message', algorithm: 'btree', columns: ['messageId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

// Read receipts - tracks last-read message per user per room
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
    readAt: t.timestamp(),
  }
);

// Typing indicators
const typingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [{ accessor: 'by_room', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    expiresAt: t.timestamp(),
  }
);

// Scheduled messages
const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    indexes: [{ accessor: 'by_author', algorithm: 'btree', columns: ['authorIdentity'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    authorIdentity: t.identity(),
    text: t.string(),
    sendAt: t.timestamp(),
    cancelled: t.bool(),
  }
);

const spacetimedb = schema({
  user,
  room,
  roomMember,
  message,
  messageEdit,
  reaction,
  readReceipt,
  typingIndicator,
  scheduledMessage,
});

export default spacetimedb;
export { user, room, roomMember, message, messageEdit, reaction, readReceipt, typingIndicator, scheduledMessage };
