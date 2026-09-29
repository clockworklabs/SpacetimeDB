import {
  schema,
  t,
  Router,
  SenderError,
  type TransactionCtx,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import * as agents from '@spacetimedb/agents/submodule';
import * as auth from '@spacetimedb/auth/submodule';
import { consumeRateLimit } from '@spacetimedb/rate-limit/submodule';
import * as agentRateLimit from '@spacetimedb/rate-limit/submodule';
import * as files from '@spacetimedb/files/submodule';
import { agents as agentDefinitions } from './agents';
import { attachmentValidationError } from './attachments';
import { messageAttachment, tokenLimit } from './model';
import { registerAgentViews } from './views';

const U32_MAX = 0xffff_ffff;
const AGENT_TOKEN_RATE_LIMIT_SCOPE = 'agents.tokens';

function throwSenderError(msg: string): never {
  throw new SenderError(msg);
}

// Development mailer that logs messages. Configure a delivery provider in production.
const consoleSendMail: auth.SendMailFn = (_ctx, params) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

const spacetimedb = schema({
  auth,
  files,
  agentRateLimit,
  agents,
  tokenLimit,
  messageAttachment,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type WriteCtx = TransactionCtx<Schema>;

export const { myThreads, myMessages, myThreadLocks, myFiles } =
  registerAgentViews(spacetimedb);

// Procedures and reducers both expose sender and db.
type CallerCtx = ProcedureCtx<Schema> | ReducerCtx<Schema>;

// Threads are owned by the auth userId so the same user works across devices.
function requireUserId(ctx: CallerCtx): string {
  const userId = auth.getCallerUserId(ctx.as.auth);
  if (!userId) throwSenderError('agent.not_authenticated');
  return userId;
}

function rateLimitKey(userId: string): string {
  return `${AGENT_TOKEN_RATE_LIMIT_SCOPE}:${userId}`;
}

function checkRateLimit(tx: WriteCtx, userId: string): void {
  const limit = tx.db.tokenLimit.singleton.find(true);
  if (!limit) return;
  const bucket = tx.db.agentRateLimit.rateLimitBucket.key.find(
    rateLimitKey(userId)
  );
  if (
    bucket &&
    bucket.expiresAt.microsSinceUnixEpoch > tx.timestamp.microsSinceUnixEpoch &&
    bucket.count >= limit.tokensPerWindow
  ) {
    throwSenderError(
      `agent.rate_limited:${bucket.count}/${limit.tokensPerWindow}`
    );
  }
}

function bumpRateLimit(tx: WriteCtx, userId: string, tokens: number): void {
  const limit = tx.db.tokenLimit.singleton.find(true);
  if (!limit) return;
  const result = consumeRateLimit(tx.as.agentRateLimit, {
    key: rateLimitKey(userId),
    scope: AGENT_TOKEN_RATE_LIMIT_SCOPE,
    // The cap is enforced before each run; this only records usage.
    limit: U32_MAX,
    windowSeconds: limit.windowSecs,
    cost: Math.min(tokens, U32_MAX),
  });
  if (!result.allowed) throwSenderError('agent.rate_limit_counter_overflow');
}

const BASE64_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function bytesToBase64(bytes: ArrayLike<number>): string {
  let output = '';
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index]!;
    const second = index + 1 < bytes.length ? bytes[index + 1]! : 0;
    const third = index + 2 < bytes.length ? bytes[index + 2]! : 0;
    output += BASE64_ALPHABET[first >> 2];
    output += BASE64_ALPHABET[((first & 0x03) << 4) | (second >> 4)];
    output +=
      index + 1 < bytes.length
        ? BASE64_ALPHABET[((second & 0x0f) << 2) | (third >> 6)]
        : '=';
    output += index + 2 < bytes.length ? BASE64_ALPHABET[third & 0x3f] : '=';
  }
  return output;
}

const agentsClient = agents.client<WriteCtx>({
  agents: agentDefinitions,
  submodule: tx => tx.as.agents,
  beforeRun: (tx, run) => checkRateLimit(tx, run.owner),
  onUsage: (tx, usage) =>
    bumpRateLimit(tx, usage.owner, usage.promptTokens + usage.completionTokens),
  attachments: (tx, messageId) =>
    [...tx.db.messageAttachment.messageId.filter(messageId)]
      .sort((a, b) => a.ordinal - b.ordinal)
      .flatMap(row => {
        const file = tx.db.files.file.id.find(row.fileId);
        const blob = tx.db.files.fileBlob.fileId.find(row.fileId);
        return file && blob
          ? [{ mimeType: file.mimeType, data: bytesToBase64(blob.bytes) }]
          : [];
      }),
});

export const init = spacetimedb.init(ctx => {
  auth.install(ctx.as.auth);
  agentRateLimit.install(ctx.as.agentRateLimit);
  agents.install(ctx.as.agents);
});

const authHttp = auth.client({
  sendMail: consoleSendMail,
  appName: 'Agents',
  emailVerifiedRedirect: '/?verified=1',
});

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  authHttp.passwordSignup(ctx.as.auth, req)
);
export const authPasswordLogin = spacetimedb.httpHandler((ctx, req) =>
  authHttp.passwordLogin(ctx.as.auth, req)
);
export const authMe = spacetimedb.httpHandler((ctx, req) =>
  authHttp.me(ctx.as.auth, req)
);
export const authLogout = spacetimedb.httpHandler((ctx, req) =>
  authHttp.logout(ctx.as.auth, req)
);
export const authRefresh = spacetimedb.httpHandler((ctx, req) =>
  authHttp.refresh(ctx.as.auth, req)
);
export const authGoogleStart = spacetimedb.httpHandler((ctx, req) =>
  authHttp.googleStart(ctx.as.auth, req)
);
export const authGoogleCallback = spacetimedb.httpHandler((ctx, req) =>
  authHttp.googleCallback(ctx.as.auth, req)
);
export const authGithubStart = spacetimedb.httpHandler((ctx, req) =>
  authHttp.githubStart(ctx.as.auth, req)
);
export const authGithubCallback = spacetimedb.httpHandler((ctx, req) =>
  authHttp.githubCallback(ctx.as.auth, req)
);
export const authPasswordForgot = spacetimedb.httpHandler((ctx, req) =>
  authHttp.forgotPassword(ctx.as.auth, req)
);
export const authPasswordReset = spacetimedb.httpHandler((ctx, req) =>
  authHttp.resetPassword(ctx.as.auth, req)
);
export const authEmailVerifyRequest = spacetimedb.httpHandler((ctx, req) =>
  authHttp.emailVerifyRequest(ctx.as.auth, req)
);
export const authEmailVerify = spacetimedb.httpHandler((ctx, req) =>
  authHttp.emailVerify(ctx.as.auth, req)
);

// Attachments are owner-only; the request's session cookie identifies the owner.
export const fileServe = spacetimedb.httpHandler((ctx, req) => {
  const userId = ctx.withTx(tx => auth.requestUserId(tx.as.auth, req));
  return files.serveFile(
    ctx.as.files,
    req,
    file => userId !== undefined && file.ownerUserId === userId
  );
});

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

// Admin-gated; both values set enables the cap, both unset removes it.
export const setTokenLimit = spacetimedb.reducer(
  {
    tokensPerWindow: t.option(t.u32()),
    windowSecs: t.option(t.u32()),
  },
  (ctx, args) => {
    if (!agentsClient.isAdmin(ctx)) throwSenderError('agent.not_authorized');
    if (args.tokensPerWindow === undefined && args.windowSecs === undefined) {
      ctx.db.tokenLimit.singleton.delete(true);
      return;
    }
    if (!args.tokensPerWindow || !args.windowSecs) {
      throwSenderError('agent.invalid_token_limit');
    }
    const row = {
      singleton: true,
      tokensPerWindow: args.tokensPerWindow,
      windowSecs: args.windowSecs,
      updatedAt: ctx.timestamp,
    };
    if (ctx.db.tokenLimit.singleton.find(true)) {
      ctx.db.tokenLimit.singleton.update(row);
    } else {
      ctx.db.tokenLimit.insert(row);
    }
  }
);

export const getAgentConfigStatus = spacetimedb.procedure(
  {},
  t.object('AgentConfigStatus', {
    isConfigured: t.bool(),
    staleLockThresholdSecs: t.u32(),
    rateLimitTokensPerWindow: t.option(t.u32()),
    rateLimitWindowSecs: t.option(t.u32()),
    agents: t.array(
      t.object('AgentInfo', {
        name: t.string(),
        model: t.string(),
        models: t.array(t.string()),
      })
    ),
    configuredProviders: t.array(t.string()),
  }),
  ctx =>
    ctx.withTx(tx => {
      const status = agentsClient.status(tx);
      const limit = tx.db.tokenLimit.singleton.find(true);
      return {
        ...status,
        isConfigured: status.configuredProviders.length > 0,
        rateLimitTokensPerWindow: limit?.tokensPerWindow,
        rateLimitWindowSecs: limit?.windowSecs,
      };
    })
);

export const startThread = spacetimedb.procedure(
  {
    agentName: t.string(),
    title: t.option(t.string()),
    metadata: t.option(t.string()),
  },
  t.u64(),
  (ctx, args) => {
    const owner = requireUserId(ctx);
    return ctx.withTx(tx => agentsClient.startThread(tx, { owner, ...args }));
  }
);

export const updateThread = spacetimedb.reducer(
  {
    threadId: t.u64(),
    title: t.option(t.string()),
    modelOverride: t.option(t.string()),
    metadata: t.option(t.string()),
    clearTitle: t.bool(),
    clearModelOverride: t.bool(),
    clearMetadata: t.bool(),
  },
  (ctx, args) => {
    agentsClient.updateThread(ctx, { owner: requireUserId(ctx), ...args });
  }
);

export const deleteThread = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const owner = requireUserId(ctx);
    agentsClient.deleteThread(ctx, { owner, threadId });
    for (const a of [...ctx.db.messageAttachment.threadId.filter(threadId)]) {
      const file = ctx.db.files.file.id.find(a.fileId);
      if (file) files.deleteFile(ctx.as.files, { path: file.path }, owner);
      ctx.db.messageAttachment.delete(a);
    }
  }
);

// No-op if the thread already has a title.
export const generateThreadTitle = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    agentsClient.generateThreadTitle(ctx, {
      owner: requireUserId(ctx),
      threadId,
    });
    return {};
  }
);

export const requestCancel = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    agentsClient.requestCancel(ctx, { owner: requireUserId(ctx), threadId });
  }
);

export const sendMessage = spacetimedb.procedure(
  {
    threadId: t.u64(),
    content: t.string(),
    attachments: t.array(
      t.object('SendAttachment', {
        mimeType: t.string(),
        filename: t.option(t.string()),
        bytes: t.array(t.u8()),
      })
    ),
  },
  t.unit(),
  (ctx, args) => {
    if (args.content.length === 0 && args.attachments.length === 0) {
      throwSenderError('agent.empty_message');
    }
    const attachmentError = attachmentValidationError(args.attachments);
    if (attachmentError) throwSenderError(attachmentError);
    const owner = requireUserId(ctx);

    agentsClient.sendMessage(ctx, {
      owner,
      threadId: args.threadId,
      content: args.content,
      onInsert: (tx, messageId) => {
        args.attachments.forEach((a, ordinal) => {
          const fileId = files.uploadFile(
            tx.as.files,
            {
              path: `/msg/${messageId}/${ordinal}`,
              mimeType: a.mimeType,
              bytes: a.bytes,
              visibility: files.FILE_VISIBILITY_OWNER,
            },
            owner
          );
          tx.db.messageAttachment.insert({
            id: 0n,
            fileId,
            messageId,
            threadId: args.threadId,
            ownerUserId: owner,
            ordinal,
            filename: a.filename,
            createdAt: tx.timestamp,
          });
        });
      },
    });
    return {};
  }
);

export const regenerateResponse = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    agentsClient.regenerateResponse(ctx, {
      owner: requireUserId(ctx),
      threadId,
    });
    return {};
  }
);
