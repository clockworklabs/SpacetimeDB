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
import { Timestamp } from 'spacetimedb';
import * as agents from '@spacetimedb/agents/submodule';
import * as auth from '@spacetimedb/auth/submodule';
import { linkConnection as bindAuthConnection } from '@spacetimedb/auth';
import {
  setAuthConfigParams,
  getPublicKeyPemParams,
  linkConnectionParams,
  unlinkConnectionParams,
  updateProfileParams,
  revokeSessionParams,
  listMySessionsParams,
  revokeMySessionParams,
  passwordSignupHandler,
  parseCookies,
  passwordLoginHandler,
  meHandler,
  logoutHandler,
  refreshHandler,
  googleStartHandler,
  googleCallbackHandler,
  githubStartHandler,
  githubCallbackHandler,
  makeForgotPasswordHandler,
  resetPasswordHandler,
  makeEmailVerifyRequestHandler,
  makeEmailVerifyHandler,
  getCallerUserId,
  publicKeyFromPem,
  verifyJwt,
  type SendMailFn,
  type MailParams,
} from '@spacetimedb/auth/submodule';
import { consumeRateLimit } from '@spacetimedb/rate-limit/submodule';
import * as agentRateLimit from '@spacetimedb/rate-limit/submodule';
import {
  FILE_VISIBILITY_OWNER,
  fileSha256Hex,
} from '@spacetimedb/files/submodule';
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
const consoleSendMail: SendMailFn = (_ctx, params: MailParams) => {
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

export const { myThreads, myMessages, myThreadLocks, myFiles, myAuthUser } =
  registerAgentViews(spacetimedb);

// Procedures and reducers both expose sender and db.
type CallerCtx = ProcedureCtx<Schema> | ReducerCtx<Schema>;

// Threads are owned by the auth userId so the same user works across devices.
function requireUserId(ctx: CallerCtx): string {
  const userId = getCallerUserId(ctx.as.auth);
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

export const setAuthConfig = spacetimedb.reducer(
  setAuthConfigParams,
  (ctx, args) => {
    auth.setAuthConfig(ctx.as.auth, args);
  }
);

export const getAuthPublicKey = spacetimedb.procedure(
  getPublicKeyPemParams,
  t.object('AuthPubKey', {
    publicKeyPem: t.string(),
    keyId: t.string(),
    issuerUrl: t.string(),
  }),
  (ctx, args) =>
    auth.getAuthPublicKey(ctx.as.auth, args) as {
      publicKeyPem: string;
      keyId: string;
      issuerUrl: string;
    }
);

// Procedure (not reducer) so the client can await commit before subscribing.
export const linkConnection = spacetimedb.procedure(
  linkConnectionParams,
  t.object('LinkConnectionResult', { userId: t.string() }),
  (ctx, args) => bindAuthConnection(ctx.as.auth, args)
);

export const unlinkConnection = spacetimedb.reducer(
  unlinkConnectionParams,
  (ctx, args) => {
    auth.unlinkConnection(ctx.as.auth, args);
  }
);

export const updateProfile = spacetimedb.reducer(
  updateProfileParams,
  (ctx, args) => {
    auth.updateProfile(ctx.as.auth, args);
  }
);

export const revokeSession = spacetimedb.reducer(
  revokeSessionParams,
  (ctx, args) => {
    auth.revokeSession(ctx.as.auth, args);
  }
);

export const listMySessions = spacetimedb.procedure(
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
  (ctx, args) =>
    auth.listMySessions(ctx.as.auth, args) as {
      sessions: Array<{
        sessionId: string;
        expiresAt: Timestamp;
        createdAt: Timestamp;
        ipAddress: string | undefined;
        userAgent: string | undefined;
        isCurrent: boolean;
      }>;
    }
);

export const revokeMySession = spacetimedb.reducer(
  revokeMySessionParams,
  (ctx, args) => {
    auth.revokeMySession(ctx.as.auth, args);
  }
);

const forgotHandler = makeForgotPasswordHandler({
  sendMail: consoleSendMail,
  appName: 'Agents',
});
const verifyRequestHandler = makeEmailVerifyRequestHandler({
  sendMail: consoleSendMail,
  appName: 'Agents',
});
const verifyHandler = makeEmailVerifyHandler({
  successRedirect: '/?verified=1',
});
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

const fileServeHandler = files.createFileHttpHandler({
  getOwner: (ctx, req) =>
    ctx.withTx((tx: TransactionCtx<Schema>) => {
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
    ctx.withTx((tx: TransactionCtx<Schema>) => {
      if (!userId) return false;
      if (file.ownerUserId === userId) return true;
      for (const a of tx.db.messageAttachment.fileId.filter(file.id)) {
        if (a.ownerUserId === userId) return true;
      }
      return false;
    }),
});
export const fileServe = spacetimedb.httpHandler(fileServeHandler);

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
    agentsClient.deleteThread(ctx, { owner: requireUserId(ctx), threadId });
    for (const a of [...ctx.db.messageAttachment.threadId.filter(threadId)]) {
      ctx.db.files.fileBlob.fileId.delete(a.fileId);
      ctx.db.files.file.id.delete(a.fileId);
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
          const path = `/msg/${messageId}/${ordinal}`;
          const file = tx.db.files.file.insert({
            id: 0n,
            ownerPathKey: files.ownerPathKey(owner, path),
            path,
            ownerUserId: owner,
            mimeType: a.mimeType,
            size: BigInt(a.bytes.length),
            sha256Hex: fileSha256Hex(a.bytes),
            visibility: FILE_VISIBILITY_OWNER,
            createdAt: tx.timestamp,
            updatedAt: tx.timestamp,
          });
          tx.db.files.fileBlob.insert({ fileId: file.id, bytes: a.bytes });
          tx.db.messageAttachment.insert({
            id: 0n,
            fileId: file.id,
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
