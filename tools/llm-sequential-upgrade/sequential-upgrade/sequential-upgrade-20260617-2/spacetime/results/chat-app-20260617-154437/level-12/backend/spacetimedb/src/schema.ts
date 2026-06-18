import { schema, table, t } from 'spacetimedb/server';

const user = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    online: t.bool(),
    status: t.string(),
    lastActiveAt: t.option(t.timestamp()),
    isAnonymous: t.bool(),
  }
);

const room = table(
  { name: 'room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAt: t.timestamp(),
    isPrivate: t.bool(),
    isDm: t.bool(),
  }
);

const roomMember = table(
  { name: 'room_member', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity(),
  }
);

const message = table(
  { name: 'message', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    senderIdentity: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
    expiresAtUs: t.option(t.u64()),
    editedAt: t.option(t.timestamp()),
    parentId: t.option(t.u64()),
  }
);

const messageEdit = table(
  { name: 'message_edit', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    editedAt: t.timestamp(),
    previousText: t.string(),
  }
);

const typingIndicator = table(
  { name: 'typing_indicator', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity(),
    updatedAt: t.timestamp(),
  }
);

const readReceipt = table(
  { name: 'read_receipt', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    userIdentity: t.identity(),
    lastReadMessageId: t.u64(),
  }
);

const scheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    scheduled: (): any => sendScheduledMessage,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    roomId: t.u64(),
    senderIdentity: t.identity(),
    text: t.string(),
  }
);

const messageExpiry = table(
  {
    name: 'message_expiry',
    public: false,
    scheduled: (): any => deleteExpiredMessage,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    messageId: t.u64(),
  }
);

const messageReaction = table(
  { name: 'message_reaction', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index('btree'),
    userIdentity: t.identity(),
    emoji: t.string(),
  }
);

const roomPermission = table(
  {
    name: 'room_permission',
    public: true,
    indexes: [{ accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    userIdentity: t.identity(),
    isAdmin: t.bool(),
    isBanned: t.bool(),
  }
);

const activeConnection = table(
  {
    name: 'active_connection',
    public: false,
    indexes: [{ accessor: 'byIdentity', algorithm: 'btree', columns: ['userIdentity'] }],
  },
  {
    connectionId: t.connectionId().primaryKey(),
    userIdentity: t.identity(),
  }
);

const roomInvitation = table(
  {
    name: 'room_invitation',
    public: true,
    indexes: [{ accessor: 'byInvitee', algorithm: 'btree', columns: ['inviteeIdentity'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    inviterIdentity: t.identity(),
    inviteeIdentity: t.identity(),
    invitedAt: t.timestamp(),
    status: t.string(),
  }
);

const messageDraft = table(
  {
    name: 'message_draft',
    public: true,
    indexes: [{ accessor: 'byUserRoom', algorithm: 'btree', columns: ['userIdentity', 'roomId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    userIdentity: t.identity(),
    roomId: t.u64(),
    text: t.string(),
    updatedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ user, room, roomMember, message, typingIndicator, readReceipt, scheduledMessage, messageExpiry, messageReaction, messageEdit, roomPermission, activeConnection, roomInvitation, messageDraft });
export default spacetimedb;

export const sendScheduledMessage = spacetimedb.reducer(
  { timer: scheduledMessage.rowType },
  (ctx, { timer }) => {
    ctx.db.message.insert({
      id: 0n,
      roomId: timer.roomId,
      senderIdentity: timer.senderIdentity,
      text: timer.text,
      sentAt: ctx.timestamp,
      expiresAtUs: undefined,
      editedAt: undefined,
      parentId: undefined,
    });
  }
);

export const deleteExpiredMessage = spacetimedb.reducer(
  { timer: messageExpiry.rowType },
  (ctx, { timer }) => {
    ctx.db.message.id.delete(timer.messageId);
  }
);
