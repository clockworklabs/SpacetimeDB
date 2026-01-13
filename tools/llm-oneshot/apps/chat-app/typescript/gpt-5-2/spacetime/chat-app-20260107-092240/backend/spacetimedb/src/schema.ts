import { schema, table, t } from 'spacetimedb/server';

export const User = table(
  {
    name: 'user',
    public: true,
    indexes: [
      { name: 'by_online', algorithm: 'btree', columns: ['online'] },
      { name: 'by_last_seen_micros', algorithm: 'btree', columns: ['lastSeenMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    identity: t.identity().unique(),
    displayName: t.string(),
    online: t.bool(),
    createdAtMicros: t.u64(),
    lastSeenMicros: t.u64(),
    lastMessageMicros: t.u64(),
    lastTypingMicros: t.u64(),
  },
);

export const Room = table(
  {
    name: 'room',
    public: true,
    indexes: [{ name: 'by_created_at_micros', algorithm: 'btree', columns: ['createdAtMicros'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    createdBy: t.identity(),
    createdAtMicros: t.u64(),
  },
);

export const RoomMember = table(
  {
    name: 'room_member',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    joinedAtMicros: t.u64(),
    isAdmin: t.bool(),
  },
);

export const Message = table(
  {
    name: 'message',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_author', algorithm: 'btree', columns: ['author'] },
      { name: 'by_created_at_micros', algorithm: 'btree', columns: ['createdAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    author: t.identity(),
    content: t.string(),
    createdAtMicros: t.u64(),
    editedAtMicros: t.u64().optional(),
    isEphemeral: t.bool(),
    expiresAtMicros: t.u64().optional(),
  },
);

export const MessageEdit = table(
  {
    name: 'message_edit',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_edited_at_micros', algorithm: 'btree', columns: ['editedAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    editor: t.identity(),
    oldContent: t.string(),
    newContent: t.string(),
    editedAtMicros: t.u64(),
  },
);

export const ScheduledMessage = table(
  {
    name: 'scheduled_message',
    public: true,
    indexes: [
      { name: 'by_author', algorithm: 'btree', columns: ['author'] },
      { name: 'by_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_scheduled_at_micros', algorithm: 'btree', columns: ['scheduledAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    author: t.identity(),
    content: t.string(),
    createdAtMicros: t.u64(),
    scheduledAtMicros: t.u64(),
    jobId: t.u64(),
  },
);

export const ScheduledMessageJob = table(
  {
    name: 'scheduled_message_job',
    scheduled: 'send_scheduled_message',
    indexes: [{ name: 'by_scheduled_message_id', algorithm: 'btree', columns: ['scheduledMessageId'] }],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    scheduledMessageId: t.u64(),
  },
);

export const EphemeralMessageCleanup = table(
  {
    name: 'ephemeral_message_cleanup',
    scheduled: 'delete_ephemeral_message',
    indexes: [{ name: 'by_message_id', algorithm: 'btree', columns: ['messageId'] }],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    messageId: t.u64(),
  },
);

export const TypingIndicator = table(
  {
    name: 'typing_indicator',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_identity', algorithm: 'btree', columns: ['identity'] },
      { name: 'by_expires_at_micros', algorithm: 'btree', columns: ['expiresAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    expiresAtMicros: t.u64(),
  },
);

export const TypingIndicatorJob = table(
  {
    name: 'typing_indicator_job',
    scheduled: 'expire_typing_indicator',
    indexes: [{ name: 'by_typing_indicator_id', algorithm: 'btree', columns: ['typingIndicatorId'] }],
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    typingIndicatorId: t.u64(),
    expiresAtMicros: t.u64(),
  },
);

export const ReadReceipt = table(
  {
    name: 'read_receipt',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_identity', algorithm: 'btree', columns: ['identity'] },
      { name: 'by_read_at_micros', algorithm: 'btree', columns: ['readAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    identity: t.identity(),
    readAtMicros: t.u64(),
  },
);

export const RoomReadPosition = table(
  {
    name: 'room_read_position',
    public: true,
    indexes: [
      { name: 'by_room_id', algorithm: 'btree', columns: ['roomId'] },
      { name: 'by_identity', algorithm: 'btree', columns: ['identity'] },
      { name: 'by_last_read_at_micros', algorithm: 'btree', columns: ['lastReadAtMicros'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    identity: t.identity(),
    lastReadMessageId: t.u64(),
    lastReadAtMicros: t.u64(),
  },
);

export const Reaction = table(
  {
    name: 'reaction',
    public: true,
    indexes: [
      { name: 'by_message_id', algorithm: 'btree', columns: ['messageId'] },
      { name: 'by_identity', algorithm: 'btree', columns: ['identity'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64(),
    identity: t.identity(),
    emoji: t.string(),
    createdAtMicros: t.u64(),
  },
);

export const spacetimedb = schema(
  User,
  Room,
  RoomMember,
  Message,
  MessageEdit,
  ScheduledMessage,
  ScheduledMessageJob,
  EphemeralMessageCleanup,
  TypingIndicator,
  TypingIndicatorJob,
  ReadReceipt,
  RoomReadPosition,
  Reaction,
);

