import { schema, table, t } from 'spacetimedb/server';

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

const spacetimedb = schema({ user, room, membership, message, typingIndicator, readReceipt });
export default spacetimedb;
