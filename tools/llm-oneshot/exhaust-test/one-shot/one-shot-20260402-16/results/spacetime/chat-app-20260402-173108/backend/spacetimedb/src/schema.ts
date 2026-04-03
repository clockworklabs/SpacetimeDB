import { table, t } from 'spacetimedb/server';

export const user = table({ name: 'user', public: true }, {
  identity: t.identity().primaryKey(),
  name: t.string(),
  online: t.bool(),
  status: t.string(), // 'online', 'away', 'dnd', 'invisible'
  lastActiveMicros: t.u64(),
});

export const room = table({ name: 'room', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  createdBy: t.identity(),
});

export const roomMember = table({ name: 'room_member', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64().index('btree'),
  userId: t.identity(),
  isAdmin: t.bool(),
  isBanned: t.bool(),
});

export const message = table({ name: 'message', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64().index('btree'),
  sender: t.identity(),
  text: t.string(),
  sentAtMicros: t.u64(),
  isEdited: t.bool(),
  isEphemeral: t.bool(),
  expiresAtMicros: t.u64(), // 0 if not ephemeral
});

export const messageEditHistory = table({ name: 'message_edit_history', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  messageId: t.u64().index('btree'),
  text: t.string(),
  editedAtMicros: t.u64(),
});

export const typingIndicator = table({ name: 'typing_indicator', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64().index('btree'),
  userId: t.identity(),
  expiresAtMicros: t.u64(),
});

export const lastRead = table({ name: 'last_read', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  roomId: t.u64().index('btree'),
  userId: t.identity(),
  lastMessageId: t.u64(),
});

export const reaction = table({ name: 'reaction', public: true }, {
  id: t.u64().primaryKey().autoInc(),
  messageId: t.u64().index('btree'),
  userId: t.identity(),
  emoji: t.string(),
});
