import { t, type InferSchema, type ViewCtx } from 'spacetimedb/server';
import {
  fileViewRow,
  message,
  messageEmbedding,
  thread,
  threadLock,
} from './model';

const authUserViewRow = t.object('AgentAuthUser', {
  userId: t.string(),
  email: t.string(),
  emailVerified: t.bool(),
  name: t.option(t.string()),
  image: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
});

export function registerAgentViews(
  spacetimedb: typeof import('./index').default
) {
  type Schema = InferSchema<typeof spacetimedb>;
  const callerUserId = (ctx: ViewCtx<Schema>) =>
    ctx.db.auth.authConnectionBinding.stdbIdentity.find(ctx.sender)?.userId;

  const myThreads = spacetimedb.view(
    { name: 'my_threads', public: true },
    t.array(thread.rowType),
    ctx => {
      const userId = callerUserId(ctx);
      return userId ? [...ctx.db.thread.userId.filter(userId)] : [];
    }
  );

  const myMessages = spacetimedb.view(
    { name: 'my_messages', public: true },
    t.array(message.rowType),
    ctx => {
      const userId = callerUserId(ctx);
      return userId ? [...ctx.db.message.userId.filter(userId)] : [];
    }
  );

  const myThreadLocks = spacetimedb.view(
    { name: 'my_thread_locks', public: true },
    t.array(threadLock.rowType),
    ctx => {
      const userId = callerUserId(ctx);
      return userId ? [...ctx.db.threadLock.userId.filter(userId)] : [];
    }
  );

  const myMessageEmbeddings = spacetimedb.view(
    { name: 'my_message_embeddings', public: true },
    t.array(messageEmbedding.rowType),
    ctx => {
      const userId = callerUserId(ctx);
      return userId ? [...ctx.db.messageEmbedding.userId.filter(userId)] : [];
    }
  );

  const myFiles = spacetimedb.view(
    { name: 'my_files', public: true },
    t.array(fileViewRow),
    ctx => {
      const userId = callerUserId(ctx);
      if (!userId) return [];
      const rows = [];
      for (const attachment of ctx.db.messageAttachment.ownerUserId.filter(
        userId
      )) {
        const file = ctx.db.files.file.id.find(attachment.fileId);
        if (!file) continue;
        rows.push({
          id: attachment.id,
          fileId: attachment.fileId,
          path: file.path,
          ownerUserId: attachment.ownerUserId,
          mimeType: file.mimeType,
          size: file.size,
          sha256Hex: file.sha256Hex,
          visibility: file.visibility,
          filename: attachment.filename,
          messageId: attachment.messageId,
          threadId: attachment.threadId,
          ordinal: attachment.ordinal,
          createdAt: attachment.createdAt,
          updatedAt: file.updatedAt,
        });
      }
      return rows;
    }
  );

  const myAuthUser = spacetimedb.view(
    { name: 'my_auth_user', public: true },
    t.array(authUserViewRow),
    ctx => {
      const userId = callerUserId(ctx);
      if (!userId) return [];
      const row = ctx.db.auth.authUser.userId.find(userId);
      return row ? [row] : [];
    }
  );

  return {
    myThreads,
    myMessages,
    myThreadLocks,
    myMessageEmbeddings,
    myFiles,
    myAuthUser,
  };
}
