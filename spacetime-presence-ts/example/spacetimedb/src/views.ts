import { t, type ViewCtx } from 'spacetimedb/server';
import {
  RATE_LIMIT_PROFILE,
  RATE_LIMIT_REACTION,
  RATE_LIMIT_ROOM_WRITE,
  RATE_LIMIT_SEND,
  RATE_LIMIT_TYPING,
  PRESENCE_SCOPE_GLOBAL,
  typingScope,
} from './chat-policy';
import {
  attachmentViewRow,
  chatUser,
  message,
  messageReaction,
  messageThread,
  presenceEntry,
  room,
  roomMember,
  roomReadCursor,
  server,
  serverMember,
  threadMessage,
} from './model';
import type { DbSchema } from './index';

type SpacetimeDb = typeof import('./index').default;

function myRoomIds(ctx: ViewCtx<DbSchema>): Set<bigint> {
  const out = new Set<bigint>();
  const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
    ctx.sender
  );
  if (!binding) return out;
  for (const membership of ctx.db.roomMember.userId.filter(binding.userId)) {
    out.add(membership.roomId);
  }
  return out;
}

function myServerIds(ctx: ViewCtx<DbSchema>): Set<bigint> {
  const out = new Set<bigint>();
  const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
    ctx.sender
  );
  if (!binding) return out;
  for (const membership of ctx.db.serverMember.userId.filter(binding.userId)) {
    out.add(membership.serverId);
  }
  for (const roomId of myRoomIds(ctx)) {
    const roomRow = ctx.db.room.id.find(roomId);
    if (roomRow) out.add(roomRow.serverId);
  }
  return out;
}

function myMessageIds(
  ctx: ViewCtx<DbSchema>,
  roomIds: Set<bigint>
): Set<bigint> {
  const out = new Set<bigint>();
  for (const roomId of roomIds) {
    for (const row of ctx.db.message.roomId.filter(roomId)) out.add(row.id);
  }
  return out;
}

function myThreadIds(
  ctx: ViewCtx<DbSchema>,
  roomIds: Set<bigint>
): Set<bigint> {
  const out = new Set<bigint>();
  for (const roomId of roomIds) {
    for (const row of ctx.db.messageThread.roomId.filter(roomId))
      out.add(row.id);
  }
  return out;
}

function myVisibleUserIds(
  ctx: ViewCtx<DbSchema>,
  serverIds = myServerIds(ctx),
  roomIds = myRoomIds(ctx)
): Set<string> {
  const out = new Set<string>();
  const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
    ctx.sender
  );
  if (binding) out.add(binding.userId);
  for (const serverId of serverIds) {
    for (const row of ctx.db.serverMember.serverId.filter(serverId))
      out.add(row.userId);
  }
  for (const roomId of roomIds) {
    for (const row of ctx.db.roomMember.roomId.filter(roomId))
      out.add(row.userId);
  }
  return out;
}

function myVisibleIdentitySubjects(
  ctx: ViewCtx<DbSchema>,
  userIds: Set<string>
): Set<string> {
  const out = new Set<string>();
  for (const userId of userIds) {
    for (const user of ctx.db.chatUser.userId.filter(userId)) {
      out.add(user.identity.toHexString());
    }
  }
  return out;
}

export function registerChatViews(spacetimedb: SpacetimeDb) {
  const authUserViewRow = t.object('ChatAuthUser', {
    userId: t.string(),
    email: t.string(),
    emailVerified: t.bool(),
    name: t.option(t.string()),
    image: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  });
  const rateLimitStatusRow = t.object('ChatRateLimitStatus', {
    scope: t.string(),
    limit: t.u32(),
    used: t.u32(),
    remaining: t.u32(),
    resetAt: t.timestamp(),
  });

  const myServers = spacetimedb.view(
    { name: 'my_servers', public: true },
    t.array(server.rowType),
    ctx => {
      const ids = myServerIds(ctx);
      const out = [];
      for (const serverId of ids) {
        const row = ctx.db.server.id.find(serverId);
        if (row) out.push(row);
      }
      return out;
    }
  );

  const myServerMembers = spacetimedb.view(
    { name: 'my_server_members', public: true },
    t.array(serverMember.rowType),
    ctx => {
      const out = [];
      for (const serverId of myServerIds(ctx)) {
        for (const row of ctx.db.serverMember.serverId.filter(serverId))
          out.push(row);
      }
      return out;
    }
  );

  const myChatUsers = spacetimedb.view(
    { name: 'my_chat_users', public: true },
    t.array(chatUser.rowType),
    ctx => {
      const out = [];
      for (const userId of myVisibleUserIds(ctx)) {
        for (const row of ctx.db.chatUser.userId.filter(userId)) out.push(row);
      }
      return out;
    }
  );

  const myPresenceEntries = spacetimedb.view(
    { name: 'my_presence_entries', public: true },
    t.array(presenceEntry.rowType),
    ctx => {
      const roomIds = myRoomIds(ctx);
      const visibleSubjects = myVisibleIdentitySubjects(
        ctx,
        myVisibleUserIds(ctx, myServerIds(ctx), roomIds)
      );
      const out = [];
      for (const subject of visibleSubjects) {
        for (const entry of ctx.db.presenceEntry.subject.filter(subject)) {
          if (entry.scope === PRESENCE_SCOPE_GLOBAL) out.push(entry);
        }
      }
      for (const roomId of roomIds) {
        for (const entry of ctx.db.presenceEntry.scope.filter(
          typingScope(roomId)
        )) {
          if (visibleSubjects.has(entry.subject)) out.push(entry);
        }
      }
      return out;
    }
  );

  const myRooms = spacetimedb.view(
    { name: 'my_rooms', public: true },
    t.array(room.rowType),
    ctx => {
      const out = [];
      for (const roomId of myRoomIds(ctx)) {
        const row = ctx.db.room.id.find(roomId);
        if (row) out.push(row);
      }
      return out;
    }
  );

  const myRoomMembers = spacetimedb.view(
    { name: 'my_room_members', public: true },
    t.array(roomMember.rowType),
    ctx => {
      const out = [];
      for (const roomId of myRoomIds(ctx)) {
        for (const row of ctx.db.roomMember.roomId.filter(roomId))
          out.push(row);
      }
      return out;
    }
  );

  const myRoomMessages = spacetimedb.view(
    { name: 'my_room_messages', public: true },
    t.array(message.rowType),
    ctx => {
      const out = [];
      for (const roomId of myRoomIds(ctx)) {
        for (const row of ctx.db.message.roomId.filter(roomId)) out.push(row);
      }
      return out;
    }
  );

  const myRoomMessageReactions = spacetimedb.view(
    { name: 'my_room_message_reactions', public: true },
    t.array(messageReaction.rowType),
    ctx => {
      const rooms = myRoomIds(ctx);
      const out = [];
      for (const messageId of myMessageIds(ctx, rooms)) {
        for (const row of ctx.db.messageReaction.messageId.filter(messageId))
          out.push(row);
      }
      return out;
    }
  );

  const myMessageThreads = spacetimedb.view(
    { name: 'my_message_threads', public: true },
    t.array(messageThread.rowType),
    ctx => {
      const out = [];
      for (const roomId of myRoomIds(ctx)) {
        for (const row of ctx.db.messageThread.roomId.filter(roomId))
          out.push(row);
      }
      return out;
    }
  );

  const myThreadMessages = spacetimedb.view(
    { name: 'my_thread_messages', public: true },
    t.array(threadMessage.rowType),
    ctx => {
      const rooms = myRoomIds(ctx);
      const out = [];
      for (const threadId of myThreadIds(ctx, rooms)) {
        for (const row of ctx.db.threadMessage.threadId.filter(threadId))
          out.push(row);
      }
      return out;
    }
  );

  const myRoomAttachments = spacetimedb.view(
    { name: 'my_room_attachments', public: true },
    t.array(attachmentViewRow),
    ctx => {
      const rooms = myRoomIds(ctx);
      const out = [];
      for (const messageId of myMessageIds(ctx, rooms)) {
        for (const attachment of ctx.db.attachment.messageId.filter(
          messageId
        )) {
          const file = ctx.db.files.file.id.find(attachment.fileId);
          if (!file) continue;
          out.push({
            id: attachment.id,
            messageId: attachment.messageId,
            fileId: attachment.fileId,
            ownerUserId: attachment.ownerUserId,
            ordinal: attachment.ordinal,
            filename: attachment.filename,
            path: file.path,
            mimeType: file.mimeType,
            size: file.size,
            sha256Hex: file.sha256Hex,
            visibility: file.visibility,
            createdAt: attachment.createdAt,
            updatedAt: file.updatedAt,
          });
        }
      }
      return out;
    }
  );

  const myRoomReadCursors = spacetimedb.view(
    { name: 'my_room_read_cursors', public: true },
    t.array(roomReadCursor.rowType),
    ctx => [...ctx.db.roomReadCursor.identity.filter(ctx.sender)]
  );

  const myAuthUser = spacetimedb.view(
    { name: 'my_auth_user', public: true },
    t.array(authUserViewRow),
    ctx => {
      const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
        ctx.sender
      );
      if (!binding) return [];
      const row = ctx.db.auth.authUser.userId.find(binding.userId);
      return row ? [row] : [];
    }
  );

  const myRateLimitStatus = spacetimedb.view(
    { name: 'my_rate_limit_status', public: true },
    t.array(rateLimitStatusRow),
    ctx => {
      const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
        ctx.sender
      );
      if (!binding) return [];
      const out = [];
      for (const limit of [
        RATE_LIMIT_SEND,
        RATE_LIMIT_TYPING,
        RATE_LIMIT_ROOM_WRITE,
        RATE_LIMIT_REACTION,
        RATE_LIMIT_PROFILE,
      ]) {
        const row = ctx.db.rateLimit.rateLimitBucket.key.find(
          `${limit.scope}:user:${binding.userId}`
        );
        if (!row) continue;
        out.push({
          scope: limit.scope,
          limit: limit.limit,
          used: row.count,
          remaining: Math.max(0, limit.limit - row.count),
          resetAt: row.expiresAt,
        });
      }
      return out;
    }
  );

  return {
    myServers,
    myServerMembers,
    myChatUsers,
    myPresenceEntries,
    myRooms,
    myRoomMembers,
    myRoomMessages,
    myRoomMessageReactions,
    myMessageThreads,
    myThreadMessages,
    myRoomAttachments,
    myRoomReadCursors,
    myAuthUser,
    myRateLimitStatus,
  };
}
