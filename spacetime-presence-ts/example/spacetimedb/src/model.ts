import { table, t } from 'spacetimedb/server';

export const chatUserStatus = t.enum('ChatUserStatus', [
  'Online',
  'Away',
  'Dnd',
  'Invisible',
]);
export const ChatUserStatus = {
  Online: { tag: 'Online' as const },
  Away: { tag: 'Away' as const },
  Dnd: { tag: 'Dnd' as const },
  Invisible: { tag: 'Invisible' as const },
};

export const chatUser = table(
  { name: 'chat_user', public: false },
  {
    identity: t.identity().primaryKey(),
    userId: t.string().index(),
    displayName: t.string(),
    status: chatUserStatus.index(),
    createdAt: t.timestamp().index(),
    lastActiveAt: t.timestamp().index(),
    lastMessageAt: t.timestamp(),
  }
);

export const server = table(
  { name: 'server', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().index(),
    createdByUserId: t.string().index(),
    createdAt: t.timestamp(),
  }
);

export const serverMember = table(
  { name: 'server_member', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    serverId: t.u64().index(),
    userId: t.string().index(),
    role: t.string(),
    joinedAt: t.timestamp(),
  }
);

// Chat tables are scoped per-user via views below. Clients subscribe to the
// `my_*` views; the underlying tables are private.
export const room = table(
  { name: 'room', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    serverId: t.u64().index(),
    name: t.string().index(),
    category: t.option(t.string()),
    createdByUserId: t.string().index(),
    createdAt: t.timestamp().index(),
    isPrivate: t.bool(),
    activityLabel: t.string().index(),
    activityScore: t.u32(),
    lastActivityAt: t.option(t.timestamp()),
  }
);

export const roomMember = table(
  { name: 'room_member', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index(),
    userId: t.string().index(),
    role: t.string().index(),
    joinedAt: t.timestamp(),
  }
);

export const message = table(
  { name: 'message', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index(),
    author: t.identity().index(),
    content: t.string(),
    createdAt: t.timestamp().index(),
    editedAt: t.option(t.timestamp()),
    replyToMessageId: t.option(t.u64()),
    pinnedAt: t.option(t.timestamp()),
    pinnedBy: t.option(t.identity()),
  }
);

export const messageReaction = table(
  { name: 'message_reaction', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index(),
    identity: t.identity().index(),
    emoji: t.string().index(),
    createdAt: t.timestamp(),
  }
);

export const messageThread = table(
  { name: 'message_thread', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    rootMessageId: t.u64().unique(),
    roomId: t.u64().index(),
    createdBy: t.identity().index(),
    createdAt: t.timestamp().index(),
    updatedAt: t.timestamp().index(),
  }
);

export const threadMessage = table(
  { name: 'thread_message', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    threadId: t.u64().index(),
    author: t.identity().index(),
    content: t.string(),
    createdAt: t.timestamp().index(),
    editedAt: t.option(t.timestamp()),
  }
);

export const attachment = table(
  { name: 'attachment', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    messageId: t.u64().index(),
    fileId: t.u64().index(),
    ownerUserId: t.string().index(),
    ordinal: t.u32(),
    filename: t.option(t.string()),
    createdAt: t.timestamp(),
  }
);

export const attachmentViewRow = t.object('RoomAttachment', {
  id: t.u64(),
  messageId: t.u64(),
  fileId: t.u64(),
  ownerUserId: t.string(),
  ordinal: t.u32(),
  filename: t.option(t.string()),
  path: t.string(),
  mimeType: t.string(),
  size: t.u64(),
  sha256Hex: t.string(),
  visibility: t.string(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
});

export const roomReadCursor = table(
  { name: 'room_read_cursor', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index(),
    identity: t.identity().index(),
    lastReadMessageId: t.u64(),
    lastReadAt: t.timestamp().index(),
  }
);

export const roomActivityEvent = table(
  { name: 'room_activity_event', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index(),
    createdAt: t.timestamp().index(),
  }
);

export const presenceEntry = table(
  { name: 'presence_entry', public: false },
  {
    key: t.string().primaryKey(),
    scope: t.string().index(),
    subject: t.string().index(),
    status: t.string().index(),
    activity: t.option(t.string()),
    payloadJson: t.option(t.string()),
    joinedAt: t.timestamp().index(),
    lastSeenAt: t.timestamp().index(),
    expiresAt: t.timestamp().index(),
    updatedAt: t.timestamp(),
  }
);
