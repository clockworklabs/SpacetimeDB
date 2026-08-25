import { ScheduleAt } from 'spacetimedb';
import { Router, Range, t, type TransactionCtx } from 'spacetimedb/server';
import {
  installPresenceConfig,
  removePresence,
  runPresenceSweep,
  upsertPresence,
} from '@spacetimedb/presence';
import {
  getPublicKeyPemParams,
  githubCallbackHandler,
  githubStartHandler,
  googleCallbackHandler,
  googleStartHandler,
  linkConnectionParams,
  listMySessionsParams,
  logoutHandler,
  makeEmailVerifyHandler,
  makeEmailVerifyRequestHandler,
  makeForgotPasswordHandler,
  meHandler,
  passwordLoginHandler,
  passwordSignupHandler,
  parseCookies,
  refreshHandler,
  resetPasswordHandler,
  revokeMySessionParams,
  revokeSessionParams,
  setAuthConfigParams,
  unlinkConnectionParams,
  updateProfileParams,
  publicKeyFromPem,
  verifyJwt,
  type MailParams,
  type SendMailFn,
} from '@spacetimedb/auth/submodule';
import * as auth from '@spacetimedb/auth/submodule';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import {
  fileSha256Hex,
  FILE_VISIBILITY_OWNER,
} from '@spacetimedb/files/submodule';
import * as files from '@spacetimedb/files/submodule';
import {
  RATE_LIMIT_PROFILE,
  RATE_LIMIT_REACTION,
  RATE_LIMIT_ROOM_WRITE,
  RATE_LIMIT_SEND,
  RATE_LIMIT_TYPING,
  typingScope,
} from './chat-policy';
import { registerChatViews } from './views';
import { chatSweepTick, spacetimedb, type DbSchema } from './schema';

const ONE_SECOND_MICROS = 1_000_000n;
const TYPING_TTL_SECONDS = 4;
const GLOBAL_PRESENCE_TTL_SECONDS = 35;
const CHAT_SWEEP_INTERVAL_SECONDS = 10n;
const ACTIVITY_WINDOW_SECONDS = 5 * 60;
const ACTIVITY_CLEANUP_BATCH = 1000;
const ROOM_NAME_MAX = 64;
const DISPLAY_NAME_MAX = 32;
const MESSAGE_MAX = 2000;
const ATTACHMENT_MAX_BYTES = 4_000_000;
const ATTACHMENT_MAX_COUNT = 5;
const ATTACHMENT_MIME_MAX = 128;
const ATTACHMENT_FILENAME_MAX = 256;

const ALLOWED_REACTIONS = new Set(['+1', 'heart', 'joy', 'wow', 'sad', 'fire']);

const consoleSendMail: SendMailFn = (_ctx, params: MailParams) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

// Chat presence states. The presence-ts submodule's presence_entry.status
// stays a free-form string (the submodule is consumer-agnostic); chat_user
// pins down the exact set of values this app supports.
import { chatUserStatus, message } from './model';
import {
  canModerateRoom,
  canReadAttachmentFile,
  deleteMessageTree,
  chatStatusToString,
  enforceChatRateLimit,
  ensureUser,
  findMembership,
  findServerMembership,
  identitiesEqual as eqIdentity,
  identityHex,
  insertRoom,
  normalizeText,
  removeTypingPresence,
  requireAuthenticatedUserId,
  requireMembership,
  requireRoom,
  requireRoomAdminOrOwner,
  requireServer,
  requireServerMembership,
  senderError,
  updateGlobalPresence,
  updateRoomActivity,
  upsertRoomReadCursor,
  type Tx,
} from './domain';

export default spacetimedb;

export type { DbSchema } from './schema';
export const {
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
} = registerChatViews(spacetimedb);
export const init = spacetimedb.init(ctx => {
  auth.installAuth(ctx.as.auth);
  files.installFiles(ctx.as.files);
  rateLimit.installRateLimit(ctx.as.rateLimit);
  installPresenceConfig(ctx, {
    defaultTtlSeconds: GLOBAL_PRESENCE_TTL_SECONDS,
    sweepBatch: 1000,
  });
  ctx.db.chatSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(
      CHAT_SWEEP_INTERVAL_SECONDS * ONE_SECOND_MICROS
    ),
  });
});

export const set_auth_config = spacetimedb.reducer(
  setAuthConfigParams,
  (ctx, args) => {
    auth.set_auth_config(ctx.as.auth, args);
  }
);

export const get_auth_public_key = spacetimedb.procedure(
  getPublicKeyPemParams,
  t.object('AuthPubKey', {
    publicKeyPem: t.string(),
    keyId: t.string(),
    issuerUrl: t.string(),
  }),
  (ctx, args) =>
    auth.get_auth_public_key(ctx.as.auth, args) as {
      publicKeyPem: string;
      keyId: string;
      issuerUrl: string;
    }
);

export const link_connection = spacetimedb.reducer(
  linkConnectionParams,
  (ctx, args) => {
    auth.link_connection(ctx.as.auth, args);
  }
);

export const unlink_connection = spacetimedb.reducer(
  unlinkConnectionParams,
  (ctx, args) => {
    auth.unlink_connection(ctx.as.auth, args);
  }
);

export const update_profile = spacetimedb.reducer(
  updateProfileParams,
  (ctx, args) => {
    auth.update_profile(ctx.as.auth, args);
  }
);

export const revoke_session = spacetimedb.reducer(
  revokeSessionParams,
  (ctx, args) => {
    auth.revoke_session(ctx.as.auth, args);
  }
);

export const list_my_sessions = spacetimedb.procedure(
  listMySessionsParams,
  t.object('MySessions', {
    sessions: t.array(
      t.object('MySession', {
        sessionId: t.string(),
        expiresAt: t.timestamp(),
        createdAt: t.timestamp(),
        ipAddress: t.option(t.string()),
        userAgent: t.option(t.string()),
        isCurrent: t.bool(),
      })
    ),
  }),
  (ctx, args) => auth.list_my_sessions(ctx.as.auth, args)
);

export const revoke_my_session = spacetimedb.reducer(
  revokeMySessionParams,
  (ctx, args) => {
    auth.revoke_my_session(ctx.as.auth, args);
  }
);

export const heartbeat = spacetimedb.reducer({}, ctx => {
  const userId = requireAuthenticatedUserId(ctx);
  const tx: Tx = ctx;
  const user = ensureUser(tx, userId);
  const next = { ...user, lastActiveAt: tx.timestamp };
  tx.db.chatUser.identity.update(next);
  updateGlobalPresence(tx, next);
});

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
    userDisplayName: t.option(t.string()),
    userStatus: t.option(t.string()),
  }),
  ctx =>
    ctx.withTx(tx => {
      const binding = tx.db.auth.authConnectionBinding.stdbIdentity.find(
        tx.sender
      );
      const userId = binding?.userId ?? undefined;
      const user = tx.db.chatUser.identity.find(tx.sender);
      return {
        userId,
        senderIdentityHex: identityHex(tx.sender),
        userDisplayName: user?.displayName,
        userStatus: user ? chatStatusToString(user.status) : undefined,
      };
    })
);

export const set_display_name = spacetimedb.reducer(
  { displayName: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const displayName = normalizeText(
      'display_name',
      args.displayName,
      DISPLAY_NAME_MAX
    );
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_PROFILE.scope,
      RATE_LIMIT_PROFILE.limit,
      RATE_LIMIT_PROFILE.windowSeconds
    );
    const user = ensureUser(tx, userId);
    const next = { ...user, displayName, lastActiveAt: tx.timestamp };
    tx.db.chatUser.identity.update(next);
    updateGlobalPresence(tx, next);
  }
);

export const set_status = spacetimedb.reducer(
  { status: chatUserStatus },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_PROFILE.scope,
      RATE_LIMIT_PROFILE.limit,
      RATE_LIMIT_PROFILE.windowSeconds
    );
    const user = ensureUser(tx, userId);
    const next = { ...user, status: args.status, lastActiveAt: tx.timestamp };
    tx.db.chatUser.identity.update(next);
    updateGlobalPresence(tx, next);
  }
);

export const create_server = spacetimedb.reducer(
  { name: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const name = normalizeText('server_name', args.name, ROOM_NAME_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    ensureUser(tx, userId);
    const srv = tx.db.server.insert({
      id: 0n,
      name,
      createdByUserId: userId,
      createdAt: tx.timestamp,
    });
    tx.db.serverMember.insert({
      id: 0n,
      serverId: srv.id,
      userId,
      role: 'owner',
      joinedAt: tx.timestamp,
    });
    insertRoom(tx, {
      serverId: srv.id,
      name: 'general',
      isPrivate: false,
      createdByUserId: userId,
      role: 'owner',
    });
  }
);

export const rename_server = spacetimedb.reducer(
  { serverId: t.u64(), name: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const name = normalizeText('server_name', args.name, ROOM_NAME_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    const srv = requireServer(tx, args.serverId);
    if (srv.createdByUserId !== userId) senderError('chat.not_server_owner');
    tx.db.server.id.update({ ...srv, name });
  }
);

export const delete_server = spacetimedb.reducer(
  { serverId: t.u64() },
  (ctx, { serverId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    const srv = requireServer(tx, serverId);
    if (srv.createdByUserId !== userId) senderError('chat.not_server_owner');

    for (const r of [...tx.db.room.serverId.filter(serverId)]) {
      for (const m of [...tx.db.message.roomId.filter(r.id)]) {
        deleteMessageTree(tx, m);
      }
      for (const c of [...tx.db.roomReadCursor.roomId.filter(r.id)])
        tx.db.roomReadCursor.id.delete(c.id);
      for (const mem of [...tx.db.roomMember.roomId.filter(r.id)])
        tx.db.roomMember.id.delete(mem.id);
      for (const ev of [...tx.db.roomActivityEvent.roomId.filter(r.id)])
        tx.db.roomActivityEvent.id.delete(ev.id);
      tx.db.room.id.delete(r.id);
    }
    for (const sm of [...tx.db.serverMember.serverId.filter(serverId)])
      tx.db.serverMember.id.delete(sm.id);
    tx.db.server.id.delete(serverId);
  }
);

export const join_server = spacetimedb.reducer(
  { serverId: t.u64() },
  (ctx, { serverId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    ensureUser(tx, userId);
    requireServer(tx, serverId);
    if (findServerMembership(tx, serverId, userId)) return;
    tx.db.serverMember.insert({
      id: 0n,
      serverId,
      userId,
      role: 'member',
      joinedAt: tx.timestamp,
    });
  }
);

export const leave_server = spacetimedb.reducer(
  { serverId: t.u64() },
  (ctx, { serverId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    const srv = requireServer(tx, serverId);
    if (srv.createdByUserId === userId)
      senderError('chat.owner_cannot_leave_server');
    const mem = requireServerMembership(tx, serverId, userId);
    tx.db.serverMember.id.delete(mem.id);
    for (const r of [...tx.db.room.serverId.filter(serverId)]) {
      const rm = findMembership(tx, r.id, userId);
      if (rm) tx.db.roomMember.id.delete(rm.id);
    }
  }
);

export const create_room = spacetimedb.reducer(
  {
    serverId: t.u64(),
    name: t.string(),
    isPrivate: t.bool(),
    category: t.option(t.string()),
  },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const name = normalizeText('room_name', args.name, ROOM_NAME_MAX);
    const category = args.category
      ? normalizeText('room_category', args.category, ROOM_NAME_MAX)
      : undefined;
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    ensureUser(tx, userId);
    requireServer(tx, args.serverId);
    requireServerMembership(tx, args.serverId, userId);
    insertRoom(tx, {
      serverId: args.serverId,
      name,
      category,
      isPrivate: args.isPrivate,
      createdByUserId: userId,
      role: 'owner',
    });
  }
);

export const join_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    ensureUser(tx, userId);
    const targetRoom = requireRoom(tx, roomId);
    if (targetRoom.isPrivate) senderError('chat.room_private');
    const existing = findMembership(tx, roomId, userId);
    if (existing) return;
    tx.db.roomMember.insert({
      id: 0n,
      roomId,
      userId,
      role: 'member',
      joinedAt: tx.timestamp,
    });
  }
);

export const leave_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    requireRoom(tx, roomId);
    const membership = requireMembership(tx, roomId, userId);
    tx.db.roomMember.id.delete(membership.id);
    removeTypingPresence(tx, roomId, tx.sender);
  }
);

export const rename_room = spacetimedb.reducer(
  { roomId: t.u64(), name: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const name = normalizeText('room_name', args.name, ROOM_NAME_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    const room = requireRoomAdminOrOwner(tx, args.roomId, userId);
    tx.db.room.id.update({ ...room, name });
  }
);

export const set_room_category = spacetimedb.reducer(
  { roomId: t.u64(), category: t.option(t.string()) },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const category = args.category
      ? normalizeText('room_category', args.category, ROOM_NAME_MAX)
      : undefined;
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    const room = requireRoomAdminOrOwner(tx, args.roomId, userId);
    tx.db.room.id.update({ ...room, category });
  }
);

export const set_room_privacy = spacetimedb.reducer(
  { roomId: t.u64(), isPrivate: t.bool() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    const room = requireRoomAdminOrOwner(tx, args.roomId, userId);
    tx.db.room.id.update({ ...room, isPrivate: args.isPrivate });
  }
);

export const delete_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_ROOM_WRITE.scope,
      RATE_LIMIT_ROOM_WRITE.limit,
      RATE_LIMIT_ROOM_WRITE.windowSeconds
    );
    requireRoomAdminOrOwner(tx, roomId, userId);

    for (const m of [...tx.db.message.roomId.filter(roomId)]) {
      deleteMessageTree(tx, m);
    }
    for (const c of [...tx.db.roomReadCursor.roomId.filter(roomId)])
      tx.db.roomReadCursor.id.delete(c.id);
    for (const mem of [...tx.db.roomMember.roomId.filter(roomId)])
      tx.db.roomMember.id.delete(mem.id);
    for (const ev of [...tx.db.roomActivityEvent.roomId.filter(roomId)])
      tx.db.roomActivityEvent.id.delete(ev.id);
    removePresence(tx, typingScope(roomId), identityHex(tx.sender));
    tx.db.room.id.delete(roomId);
  }
);

const attachmentInput = t.object('AttachmentInput', {
  mimeType: t.string(),
  filename: t.option(t.string()),
  bytes: t.array(t.u8()),
});

const attachmentFileResult = t.object('AttachmentFileResult', {
  filename: t.option(t.string()),
  mimeType: t.string(),
  bytes: t.array(t.u8()),
});

export const get_attachment_file = spacetimedb.procedure(
  { fileId: t.u64() },
  attachmentFileResult,
  (ctx, args) =>
    ctx.withTx(tx => {
      const binding = tx.db.auth.authConnectionBinding.stdbIdentity.find(
        tx.sender
      );
      if (!binding || !canReadAttachmentFile(tx, binding.userId, args.fileId)) {
        senderError('chat.attachment_not_found');
      }
      const file = tx.db.files.file.id.find(args.fileId);
      if (!file) senderError('chat.attachment_not_found');
      const blob = tx.db.files.fileBlob.fileId.find(args.fileId);
      if (!blob) senderError('chat.attachment_not_found');
      let filename: string | undefined;
      for (const a of tx.db.attachment.fileId.filter(args.fileId)) {
        filename = a.filename ?? undefined;
        break;
      }
      return {
        filename,
        mimeType: file.mimeType,
        bytes: blob.bytes,
      };
    })
);

export const send_message = spacetimedb.reducer(
  {
    roomId: t.u64(),
    content: t.string(),
    replyToMessageId: t.option(t.u64()),
    attachments: t.array(attachmentInput),
  },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const trimmedContent = args.content.trim().replace(/\s+/g, ' ');
    const hasAttachments = args.attachments.length > 0;
    if (!hasAttachments && trimmedContent.length === 0)
      senderError('chat.invalid_message');
    if (trimmedContent.length > MESSAGE_MAX)
      senderError('chat.message_too_long');

    if (args.attachments.length > ATTACHMENT_MAX_COUNT) {
      senderError(
        `chat.too_many_attachments:${args.attachments.length}/${ATTACHMENT_MAX_COUNT}`
      );
    }
    for (const a of args.attachments) {
      if (a.mimeType.length === 0 || a.mimeType.length > ATTACHMENT_MIME_MAX)
        senderError('chat.invalid_attachment_mime');
      if (
        a.filename !== undefined &&
        a.filename.length > ATTACHMENT_FILENAME_MAX
      )
        senderError('chat.invalid_attachment_filename');
      if (a.bytes.length === 0) senderError('chat.empty_attachment');
      if (a.bytes.length > ATTACHMENT_MAX_BYTES)
        senderError(
          `chat.attachment_too_large:${a.bytes.length}/${ATTACHMENT_MAX_BYTES}`
        );
    }

    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    const user = ensureUser(tx, userId);
    requireRoom(tx, args.roomId);
    requireMembership(tx, args.roomId, userId);

    if (args.replyToMessageId !== undefined) {
      const parent = tx.db.message.id.find(args.replyToMessageId);
      if (!parent || parent.roomId !== args.roomId)
        senderError('chat.invalid_reply_target');
    }

    const msg = tx.db.message.insert({
      id: 0n,
      roomId: args.roomId,
      author: tx.sender,
      content: trimmedContent,
      createdAt: tx.timestamp,
      editedAt: undefined,
      replyToMessageId: args.replyToMessageId,
      pinnedAt: undefined,
      pinnedBy: undefined,
    });

    for (let i = 0; i < args.attachments.length; i++) {
      const a = args.attachments[i]!;
      const path = `/room/${args.roomId}/msg/${msg.id}/${i}`;
      const file = tx.db.files.file.insert({
        id: 0n,
        ownerPathKey: files.ownerPathKey(userId, path),
        path,
        ownerUserId: userId,
        mimeType: a.mimeType,
        size: BigInt(a.bytes.length),
        sha256Hex: fileSha256Hex(a.bytes),
        visibility: FILE_VISIBILITY_OWNER,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });
      tx.db.files.fileBlob.insert({ fileId: file.id, bytes: a.bytes });
      tx.db.attachment.insert({
        id: 0n,
        messageId: msg.id,
        fileId: file.id,
        ownerUserId: userId,
        ordinal: i,
        filename: a.filename,
        createdAt: tx.timestamp,
      });
    }

    tx.db.roomActivityEvent.insert({
      id: 0n,
      roomId: args.roomId,
      createdAt: tx.timestamp,
    });
    updateRoomActivity(tx, args.roomId);
    removeTypingPresence(tx, args.roomId, tx.sender);
    const nextUser = {
      ...user,
      lastActiveAt: tx.timestamp,
      lastMessageAt: tx.timestamp,
    };
    tx.db.chatUser.identity.update(nextUser);
    updateGlobalPresence(tx, nextUser);
  }
);

export const edit_message = spacetimedb.reducer(
  { messageId: t.u64(), content: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const content = normalizeText('message', args.content, MESSAGE_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    ensureUser(tx, userId);
    const msg = tx.db.message.id.find(args.messageId);
    if (!msg) senderError('chat.message_not_found');
    requireMembership(tx, msg.roomId, userId);
    if (!eqIdentity(msg.author, tx.sender))
      senderError('chat.not_message_author');
    tx.db.message.id.update({
      ...msg,
      content,
      editedAt: tx.timestamp,
    });
  }
);

export const delete_message = spacetimedb.reducer(
  { messageId: t.u64() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    ensureUser(tx, userId);
    const msg = tx.db.message.id.find(args.messageId);
    if (!msg) senderError('chat.message_not_found');
    requireMembership(tx, msg.roomId, userId);
    if (
      !eqIdentity(msg.author, tx.sender) &&
      !canModerateRoom(tx, msg.roomId, userId)
    )
      senderError('chat.not_message_author');
    deleteMessageTree(tx, msg);
  }
);

export const send_thread_message = spacetimedb.reducer(
  { rootMessageId: t.u64(), content: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const content = normalizeText('thread_message', args.content, MESSAGE_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    const user = ensureUser(tx, userId);
    const root = tx.db.message.id.find(args.rootMessageId);
    if (!root) senderError('chat.message_not_found');
    requireMembership(tx, root.roomId, userId);

    let thread = tx.db.messageThread.rootMessageId.find(root.id);
    if (!thread) {
      thread = tx.db.messageThread.insert({
        id: 0n,
        rootMessageId: root.id,
        roomId: root.roomId,
        createdBy: tx.sender,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });
    } else {
      tx.db.messageThread.id.update({ ...thread, updatedAt: tx.timestamp });
    }

    tx.db.threadMessage.insert({
      id: 0n,
      threadId: thread.id,
      author: tx.sender,
      content,
      createdAt: tx.timestamp,
      editedAt: undefined,
    });

    const nextUser = {
      ...user,
      lastActiveAt: tx.timestamp,
      lastMessageAt: tx.timestamp,
    };
    tx.db.chatUser.identity.update(nextUser);
    updateGlobalPresence(tx, nextUser);
  }
);

export const edit_thread_message = spacetimedb.reducer(
  { threadMessageId: t.u64(), content: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const content = normalizeText('thread_message', args.content, MESSAGE_MAX);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    ensureUser(tx, userId);
    const msg = tx.db.threadMessage.id.find(args.threadMessageId);
    if (!msg) senderError('chat.thread_message_not_found');
    const thread = tx.db.messageThread.id.find(msg.threadId);
    if (!thread) senderError('chat.thread_not_found');
    requireMembership(tx, thread.roomId, userId);
    if (!eqIdentity(msg.author, tx.sender))
      senderError('chat.not_message_author');
    tx.db.threadMessage.id.update({ ...msg, content, editedAt: tx.timestamp });
  }
);

export const delete_thread_message = spacetimedb.reducer(
  { threadMessageId: t.u64() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_SEND.scope,
      RATE_LIMIT_SEND.limit,
      RATE_LIMIT_SEND.windowSeconds
    );
    ensureUser(tx, userId);
    const msg = tx.db.threadMessage.id.find(args.threadMessageId);
    if (!msg) senderError('chat.thread_message_not_found');
    const thread = tx.db.messageThread.id.find(msg.threadId);
    if (!thread) senderError('chat.thread_not_found');
    requireMembership(tx, thread.roomId, userId);
    if (
      !eqIdentity(msg.author, tx.sender) &&
      !canModerateRoom(tx, thread.roomId, userId)
    )
      senderError('chat.not_message_author');
    tx.db.threadMessage.id.delete(msg.id);
    let latestAt = thread.createdAt;
    let remaining = 0;
    for (const row of tx.db.threadMessage.threadId.filter(thread.id)) {
      remaining++;
      if (
        (row.createdAt.microsSinceUnixEpoch as bigint) >
        (latestAt.microsSinceUnixEpoch as bigint)
      )
        latestAt = row.createdAt;
    }
    if (remaining === 0) tx.db.messageThread.id.delete(thread.id);
    else tx.db.messageThread.id.update({ ...thread, updatedAt: latestAt });
  }
);

export const start_typing = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_TYPING.scope,
      RATE_LIMIT_TYPING.limit,
      RATE_LIMIT_TYPING.windowSeconds
    );
    const user = ensureUser(tx, userId);
    requireRoom(tx, roomId);
    requireMembership(tx, roomId, userId);
    upsertPresence(tx, {
      scope: typingScope(roomId),
      subject: identityHex(tx.sender),
      status: 'typing',
      activity: 'typing',
      payloadJson: JSON.stringify({
        displayName: user.displayName,
        userId: user.userId,
      }),
      ttlSeconds: TYPING_TTL_SECONDS,
    });
  }
);

export const stop_typing = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    removeTypingPresence(tx, roomId, tx.sender);
  }
);

export const mark_room_read = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    requireRoom(tx, roomId);
    requireMembership(tx, roomId, userId);
    let latestMessageId = 0n;
    let latestMicros = 0n;
    for (const msg of tx.db.message.roomId.filter(roomId)) {
      const micros = msg.createdAt.microsSinceUnixEpoch as bigint;
      if (micros > latestMicros) {
        latestMicros = micros;
        latestMessageId = msg.id;
      }
    }
    upsertRoomReadCursor(tx, roomId, latestMessageId);
  }
);

export const toggle_reaction = spacetimedb.reducer(
  { messageId: t.u64(), emoji: t.string() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const emoji = args.emoji.trim();
    if (!ALLOWED_REACTIONS.has(emoji))
      senderError('chat.invalid_reaction_emoji');
    const tx: Tx = ctx;
    enforceChatRateLimit(
      tx,
      userId,
      RATE_LIMIT_REACTION.scope,
      RATE_LIMIT_REACTION.limit,
      RATE_LIMIT_REACTION.windowSeconds
    );
    ensureUser(tx, userId);
    const msg = tx.db.message.id.find(args.messageId);
    if (!msg) senderError('chat.message_not_found');
    requireMembership(tx, msg.roomId, userId);
    for (const r of tx.db.messageReaction.messageId.filter(msg.id)) {
      if (eqIdentity(r.identity, tx.sender) && r.emoji === emoji) {
        tx.db.messageReaction.id.delete(r.id);
        return;
      }
    }
    tx.db.messageReaction.insert({
      id: 0n,
      messageId: msg.id,
      identity: tx.sender,
      emoji,
      createdAt: tx.timestamp,
    });
  }
);

export const pin_message = spacetimedb.reducer(
  { messageId: t.u64() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    const msg = tx.db.message.id.find(args.messageId);
    if (!msg) senderError('chat.message_not_found');
    requireMembership(tx, msg.roomId, userId);
    if (msg.pinnedAt) return;
    tx.db.message.id.update({
      ...msg,
      pinnedAt: tx.timestamp,
      pinnedBy: tx.sender,
    });
  }
);

export const unpin_message = spacetimedb.reducer(
  { messageId: t.u64() },
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const tx: Tx = ctx;
    const msg = tx.db.message.id.find(args.messageId);
    if (!msg) senderError('chat.message_not_found');
    requireMembership(tx, msg.roomId, userId);
    if (!msg.pinnedAt) return;
    tx.db.message.id.update({
      ...msg,
      pinnedAt: undefined,
      pinnedBy: undefined,
    });
  }
);

const SEARCH_MAX_RESULTS = 50;
const SEARCH_QUERY_MAX = 200;
export const search_messages = spacetimedb.procedure(
  { roomId: t.u64(), query: t.string() },
  t.array(message.rowType),
  (ctx, args) => {
    const userId = requireAuthenticatedUserId(ctx);
    const q = args.query.trim().slice(0, SEARCH_QUERY_MAX).toLowerCase();
    if (q.length === 0) return [];
    return ctx.withTx(tx => {
      requireMembership(tx, args.roomId, userId);
      const matches = [...tx.db.message.roomId.filter(args.roomId)].filter(m =>
        m.content.toLowerCase().includes(q)
      );
      matches.sort((a, b) =>
        b.createdAt.microsSinceUnixEpoch < a.createdAt.microsSinceUnixEpoch
          ? -1
          : 1
      );
      return matches.slice(0, SEARCH_MAX_RESULTS);
    });
  }
);

export const chat_sweep = spacetimedb.reducer(
  { onSchedule: chatSweepTick },
  { arg: chatSweepTick.rowType },
  ctx => {
    runPresenceSweep(
      ctx,
      ctx.db.presenceEntry.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      )
    );

    const cutoff =
      (ctx.timestamp.microsSinceUnixEpoch as bigint) -
      BigInt(ACTIVITY_WINDOW_SECONDS) * ONE_SECOND_MICROS;
    let deleted = 0;
    const affectedRoomIds = new Set<bigint>();
    for (const evt of ctx.db.roomActivityEvent.iter()) {
      if (deleted >= ACTIVITY_CLEANUP_BATCH) break;
      if ((evt.createdAt.microsSinceUnixEpoch as bigint) >= cutoff) continue;
      affectedRoomIds.add(evt.roomId);
      ctx.db.roomActivityEvent.delete(evt);
      deleted++;
    }

    for (const roomId of affectedRoomIds) {
      updateRoomActivity(ctx, roomId);
    }
  }
);

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  passwordSignupHandler(ctx.as.auth, req)
);
export const authPasswordLogin = spacetimedb.httpHandler((ctx, req) =>
  passwordLoginHandler(ctx.as.auth, req)
);
export const authMe = spacetimedb.httpHandler((ctx, req) =>
  meHandler(ctx.as.auth, req)
);
export const authLogout = spacetimedb.httpHandler((ctx, req) =>
  logoutHandler(ctx.as.auth, req)
);
export const authRefresh = spacetimedb.httpHandler((ctx, req) =>
  refreshHandler(ctx.as.auth, req)
);
export const authGoogleStart = spacetimedb.httpHandler((ctx, req) =>
  googleStartHandler(ctx.as.auth, req)
);
export const authGoogleCallback = spacetimedb.httpHandler((ctx, req) =>
  googleCallbackHandler(ctx.as.auth, req)
);
export const authGithubStart = spacetimedb.httpHandler((ctx, req) =>
  githubStartHandler(ctx.as.auth, req)
);
export const authGithubCallback = spacetimedb.httpHandler((ctx, req) =>
  githubCallbackHandler(ctx.as.auth, req)
);

const fileServeHandler = files.makeFileServeImpl({
  getOwner: (ctx, req) =>
    ctx.withTx((tx: TransactionCtx<DbSchema>) => {
      const binding = tx.db.auth.authConnectionBinding.stdbIdentity.find(
        tx.sender
      );
      if (binding) return binding.userId;

      const cfg = tx.db.auth.authConfig.singleton.find(true);
      if (!cfg) return undefined;
      const bearer = req.headers.get('authorization');
      const cookies = parseCookies(req.headers.get('cookie'));
      const tokens = [
        bearer && bearer.toLowerCase().startsWith('bearer ')
          ? bearer.slice(7).trim()
          : undefined,
        cookies[cfg.cookieName],
      ].filter((token): token is string => Boolean(token));
      for (const token of tokens) {
        const verified = verifyJwt(
          publicKeyFromPem(cfg.es256PublicKeyPem),
          token,
          {
            issuer: cfg.issuerUrl,
            nowSeconds: Number(
              (tx.timestamp.microsSinceUnixEpoch as bigint) / 1_000_000n
            ),
          }
        );
        if (!verified.ok || !verified.claims.jti) continue;

        const session = tx.db.auth.authSession.sessionId.find(
          verified.claims.jti
        );
        if (!session) continue;
        if (
          (session.expiresAt.microsSinceUnixEpoch as bigint) <=
          (tx.timestamp.microsSinceUnixEpoch as bigint)
        ) {
          continue;
        }
        if (session.userId === verified.claims.sub) return session.userId;
      }
      return undefined;
    }),
  canAccess: (ctx, _req, file, userId) =>
    ctx.withTx((tx: TransactionCtx<DbSchema>) => {
      if (!userId) return false;
      if (file.ownerUserId === userId) return true;
      for (const a of tx.db.attachment.fileId.filter(file.id)) {
        const msg = tx.db.message.id.find(a.messageId);
        if (!msg) continue;
        for (const member of tx.db.roomMember.roomId.filter(msg.roomId)) {
          if (member.userId === userId) return true;
        }
      }
      return false;
    }),
});
export const fileServe = spacetimedb.httpHandler(fileServeHandler);

const forgotHandler = makeForgotPasswordHandler({
  sendMail: consoleSendMail,
  appName: 'Chat',
});
const verifyRequestHandler = makeEmailVerifyRequestHandler({
  sendMail: consoleSendMail,
  appName: 'Chat',
});
const verifyHandler = makeEmailVerifyHandler({
  successRedirect: '/?verified=1',
});

export const authPasswordForgot = spacetimedb.httpHandler((ctx, req) =>
  forgotHandler(ctx.as.auth, req)
);
export const authPasswordReset = spacetimedb.httpHandler((ctx, req) =>
  resetPasswordHandler(ctx.as.auth, req)
);
export const authEmailVerifyRequest = spacetimedb.httpHandler((ctx, req) =>
  verifyRequestHandler(ctx.as.auth, req)
);
export const authEmailVerify = spacetimedb.httpHandler((ctx, req) =>
  verifyHandler(ctx.as.auth, req)
);

export const router = spacetimedb.httpRouter(
  new Router()
    .post('/auth/password/signup', authPasswordSignup)
    .post('/auth/password/login', authPasswordLogin)
    .post('/auth/session/refresh', authRefresh)
    .get('/auth/me', authMe)
    .post('/auth/logout', authLogout)
    .get('/auth/google/start', authGoogleStart)
    .get('/auth/google/callback', authGoogleCallback)
    .get('/auth/github/start', authGithubStart)
    .get('/auth/github/callback', authGithubCallback)
    .post('/auth/password/forgot', authPasswordForgot)
    .post('/auth/password/reset', authPasswordReset)
    .post('/auth/email/verify-request', authEmailVerifyRequest)
    .get('/auth/email/verify', authEmailVerify)
    .get('/files', fileServe)
    .get('/files/', fileServe)
    .head('/files/', fileServe)
    .head('/files', fileServe)
);
