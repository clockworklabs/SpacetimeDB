// Multi-agent submodule. Effective config = thread > agent_override > code default.
import {
  schema,
  table,
  t,
  Range,
  Router,
  SenderError,
  type TransactionCtx,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  deleteStaleThreadLocks,
  staleLockCutoffMicros,
} from '@spacetimedb/agents/stale-locks';
import * as auth from '@spacetimedb/auth/submodule';
import {
  setAuthConfigParams,
  getPublicKeyPemParams,
  linkConnectionParams,
  linkConnectionImpl,
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
import { callChat, type Provider } from '@spacetimedb/agents/openrouter';
import { BUILT_IN_PROVIDERS } from '@spacetimedb/agents/providers';
import {
  FILE_VISIBILITY_OWNER,
  fileSha256Hex,
} from '@spacetimedb/files/submodule';
import * as files from '@spacetimedb/files/submodule';
import { USER_CONTENT_MAX, type LoopConfig } from './loop';
import { SWEEPER_INTERVAL_MICROS } from './sweeper';
import { attachmentValidationError } from './attachments';
import { registerAgentViews } from './views';
import { maybeEmbedMessage, registry, runLockedLoop } from './runtime';

const ONE_SECOND_MICROS = 1_000_000n;
const DEFAULT_STALE_LOCK_THRESHOLD_SECS = 15 * 60;
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

// Ownership is keyed by userId so the same user works across devices.
import {
  apiKey,
  agentSecret,
  agentAdminIdentity,
  agentOverride,
  thread,
  message,
  threadLock,
  messageAttachment,
  messageEmbedding,
} from './model';

const threadLockSweeperTick = table(
  { name: 'thread_lock_sweeper_tick', scheduled: (): any => thread_lock_sweep },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({
  auth,
  files,
  agentRateLimit,
  agentSecret,
  agentAdminIdentity,
  agentOverride,
  apiKey,
  thread,
  message,
  messageAttachment,
  threadLock,
  threadLockSweeperTick,
  messageEmbedding,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type WriteCtx = TransactionCtx<Schema>;

export const {
  myThreads,
  myMessages,
  myThreadLocks,
  myMessageEmbeddings,
  myFiles,
  myAuthUser,
} = registerAgentViews(spacetimedb);

function requireAdmin(tx: WriteCtx): void {
  if (tx.db.agentAdminIdentity.identity.find(tx.sender) == null) {
    throwSenderError('agent.not_authorized');
  }
}

// Procedures and reducers both expose sender and db.
type CallerCtx = ProcedureCtx<Schema> | ReducerCtx<Schema>;

function requireUserId(ctx: CallerCtx): string {
  const userId = getCallerUserId(ctx.as.auth);
  if (!userId) throwSenderError('agent.not_authenticated');
  return userId;
}

function requireOwnedThread(tx: WriteCtx, threadId: bigint, userId: string) {
  const row = tx.db.thread.id.find(threadId);
  if (!row) throwSenderError(`agent.thread_not_found:${threadId}`);
  if (row.userId !== userId)
    throwSenderError(`agent.not_thread_owner:${threadId}`);
  return row;
}

function toU32OrThrow(name: string, value: bigint): number {
  if (value <= 0n || value > BigInt(U32_MAX)) {
    throwSenderError(`agent.invalid_${name}`);
  }
  return Number(value);
}

function rateLimitKey(userId: string): string {
  return `${AGENT_TOKEN_RATE_LIMIT_SCOPE}:${userId}`;
}

function isExpired(nowMicros: bigint, expiresAtMicros: bigint): boolean {
  return expiresAtMicros <= nowMicros;
}

function checkRateLimit(tx: WriteCtx, userId: string): void {
  const secret = tx.db.agentSecret.singleton.find(true);
  if (
    secret == null ||
    secret.rateLimitTokensPerWindow == null ||
    secret.rateLimitWindowSecs == null
  )
    return;

  const cap = Number(secret.rateLimitTokensPerWindow);
  const key = rateLimitKey(userId);
  const existing = tx.db.agentRateLimit.rateLimitBucket.key.find(key);
  if (existing == null) return;

  const nowMicros = tx.timestamp.microsSinceUnixEpoch as bigint;
  const expiresAtMicros = existing.expiresAt.microsSinceUnixEpoch as bigint;
  if (isExpired(nowMicros, expiresAtMicros)) return;

  if (existing.count >= cap) {
    throwSenderError(`agent.rate_limited:${existing.count}/${cap}`);
  }
}

function bumpRateLimit(tx: WriteCtx, userId: string, tokens: bigint): void {
  if (tokens <= 0n) return;

  const secret = tx.db.agentSecret.singleton.find(true);
  if (
    secret == null ||
    secret.rateLimitTokensPerWindow == null ||
    secret.rateLimitWindowSecs == null
  )
    return;

  const cost = toU32OrThrow('rate_limit_tokens', tokens);
  const windowSeconds = Number(secret.rateLimitWindowSecs);

  const result = consumeRateLimit(tx.as.agentRateLimit, {
    key: rateLimitKey(userId),
    scope: AGENT_TOKEN_RATE_LIMIT_SCOPE,
    // Cap enforced in checkRateLimit; this only increments usage.
    limit: U32_MAX,
    windowSeconds,
    cost,
  });

  if (!result.allowed) {
    throwSenderError('agent.rate_limit_counter_overflow');
  }
}

export const init = spacetimedb.init(ctx => {
  auth.installAuth(ctx.as.auth);
  files.installFiles(ctx.as.files);
  agentRateLimit.installRateLimit(ctx.as.agentRateLimit);
  ctx.db.agentAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
  ctx.db.threadLockSweeperTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(SWEEPER_INTERVAL_MICROS),
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

// Procedure (not reducer) so the client can await commit before subscribing.
export const link_connection = spacetimedb.procedure(
  linkConnectionParams,
  t.object('LinkConnectionResult', { userId: t.string() }),
  (ctx, args) => linkConnectionImpl(ctx.as.auth, args)
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
  (ctx, args) =>
    auth.list_my_sessions(ctx.as.auth, args) as {
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

export const revoke_my_session = spacetimedb.reducer(
  revokeMySessionParams,
  (ctx, args) => {
    auth.revoke_my_session(ctx.as.auth, args);
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

const fileServeHandler = files.makeFileServeImpl({
  getOwner: (ctx, req) =>
    ctx.withTx((tx: TransactionCtx<Schema>) => {
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

// Admin-gated tuning; API keys go through set_api_key.
export const set_agent_secret = spacetimedb.reducer(
  {
    staleLockThresholdSecs: t.option(t.u32()),
    rateLimitTokensPerWindow: t.option(t.u32()),
    rateLimitWindowSecs: t.option(t.u32()),
  },
  (ctx, args) => {
    const staleLockThresholdSecs =
      args.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS;
    if (staleLockThresholdSecs === 0) {
      throwSenderError('agent.invalid_stale_lock_threshold:must be > 0');
    }
    if (
      args.rateLimitTokensPerWindow !== undefined &&
      args.rateLimitTokensPerWindow === 0
    ) {
      throwSenderError('agent.invalid_rate_limit_tokens:must be > 0');
    }
    if (
      args.rateLimitWindowSecs !== undefined &&
      args.rateLimitWindowSecs === 0
    ) {
      throwSenderError('agent.invalid_rate_limit_window:must be > 0');
    }

    const tx = ctx;
    requireAdmin(tx);

    const existing = tx.db.agentSecret.singleton.find(true);
    const row = {
      singleton: true,
      staleLockThresholdSecs,
      rateLimitTokensPerWindow: args.rateLimitTokensPerWindow,
      rateLimitWindowSecs: args.rateLimitWindowSecs,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.agentSecret.singleton.update(row);
    } else {
      tx.db.agentSecret.insert(row);
    }
  }
);

export const set_api_key = spacetimedb.reducer(
  { provider: t.string(), key: t.string() },
  (ctx, args) => {
    if (args.provider.length === 0)
      throwSenderError('agent.invalid_provider:empty');
    if (args.key.length === 0) throwSenderError('agent.invalid_api_key:empty');
    if (!Object.hasOwn(BUILT_IN_PROVIDERS, args.provider)) {
      throwSenderError(`agent.unknown_provider:${args.provider}`);
    }
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.apiKey.provider.find(args.provider);
    const row = {
      provider: args.provider,
      key: args.key,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.apiKey.provider.update(row);
    } else {
      tx.db.apiKey.insert(row);
    }
  }
);

export const clear_api_key = spacetimedb.reducer(
  { provider: t.string() },
  (ctx, { provider }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.apiKey.provider.find(provider);
    if (existing) tx.db.apiKey.delete(existing);
  }
);

export const set_agent_override = spacetimedb.reducer(
  {
    agentName: t.string(),
    provider: t.option(t.string()),
    model: t.option(t.string()),
    systemPrompt: t.option(t.string()),
    maxTurns: t.option(t.u32()),
    maxHistoryMessages: t.option(t.u32()),
    maxTokens: t.option(t.u32()),
    retries: t.option(t.u32()),
  },
  (ctx, args) => {
    if (!registry.has(args.agentName)) {
      throwSenderError(`agent.unknown:${args.agentName}`);
    }
    if (
      args.provider !== undefined &&
      !Object.hasOwn(BUILT_IN_PROVIDERS, args.provider)
    ) {
      throwSenderError(`agent.unknown_provider:${args.provider}`);
    }
    if (args.maxTurns !== undefined && args.maxTurns === 0) {
      throwSenderError('agent.invalid_max_turns:must be > 0');
    }
    if (
      args.maxHistoryMessages !== undefined &&
      args.maxHistoryMessages === 0
    ) {
      throwSenderError('agent.invalid_max_history:must be > 0');
    }

    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentOverride.agentName.find(args.agentName);
    const row = {
      agentName: args.agentName,
      provider: args.provider,
      model: args.model,
      systemPrompt: args.systemPrompt,
      maxTurns: args.maxTurns,
      maxHistoryMessages: args.maxHistoryMessages,
      maxTokens: args.maxTokens,
      retries: args.retries,
      updatedAt: tx.timestamp,
    };
    if (existing) {
      tx.db.agentOverride.agentName.update(row);
    } else {
      tx.db.agentOverride.insert(row);
    }
  }
);

export const clear_agent_override = spacetimedb.reducer(
  { agentName: t.string() },
  (ctx, { agentName }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentOverride.agentName.find(agentName);
    if (existing) tx.db.agentOverride.delete(existing);
  }
);

export const get_agent_config_status = spacetimedb.procedure(
  {},
  t.object('AgentConfigStatus', {
    isConfigured: t.bool(),
    staleLockThresholdSecs: t.u32(),
    rateLimitTokensPerWindow: t.option(t.u32()),
    rateLimitWindowSecs: t.option(t.u32()),
    agents: t.array(
      t.object('AgentInfo', {
        name: t.string(),
        defaultProvider: t.string(),
        defaultModel: t.string(),
      })
    ),
    configuredProviders: t.array(t.string()),
  }),
  ctx =>
    ctx.withTx(tx => {
      const secret = tx.db.agentSecret.singleton.find(true);
      const configuredProviders = [...tx.db.apiKey.iter()]
        .map(r => r.provider)
        .sort();
      const agents = registry.names().map(name => {
        const def = registry.agentDef(name)!;
        return {
          name,
          defaultProvider: def.defaultProvider,
          defaultModel: def.defaultModel,
        };
      });
      return {
        isConfigured: secret != null,
        staleLockThresholdSecs:
          secret?.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS,
        rateLimitTokensPerWindow: secret?.rateLimitTokensPerWindow,
        rateLimitWindowSecs: secret?.rateLimitWindowSecs,
        agents,
        configuredProviders,
      };
    })
);

export const add_agent_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    const tx = ctx;
    requireAdmin(tx);
    if (tx.db.agentAdminIdentity.identity.find(identity) == null) {
      tx.db.agentAdminIdentity.insert({
        identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const remove_agent_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    const tx = ctx;
    requireAdmin(tx);
    const existing = tx.db.agentAdminIdentity.identity.find(identity);
    if (existing) tx.db.agentAdminIdentity.delete(existing);
  }
);

export const start_thread = spacetimedb.procedure(
  {
    agentName: t.string(),
    title: t.option(t.string()),
    systemPromptOverride: t.option(t.string()),
    metadata: t.option(t.string()),
  },
  t.u64(),
  (ctx, args) => {
    const userId = requireUserId(ctx);
    if (!registry.has(args.agentName)) {
      throwSenderError(`agent.unknown:${args.agentName}`);
    }
    return ctx.withTx(tx => {
      const inserted = tx.db.thread.insert({
        id: 0n,
        userId,
        agentName: args.agentName,
        title: args.title,
        systemPromptOverride: args.systemPromptOverride,
        modelOverride: undefined,
        metadata: args.metadata,
        summary: undefined,
        summarizedThroughId: undefined,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });
      return inserted.id;
    });
  }
);

export const update_thread = spacetimedb.reducer(
  {
    threadId: t.u64(),
    title: t.option(t.string()),
    systemPromptOverride: t.option(t.string()),
    modelOverride: t.option(t.string()),
    metadata: t.option(t.string()),
    clearTitle: t.bool(),
    clearSystemPromptOverride: t.bool(),
    clearModelOverride: t.bool(),
    clearMetadata: t.bool(),
  },
  (ctx, args) => {
    const userId = requireUserId(ctx);
    const tx = ctx;
    const row = requireOwnedThread(tx, args.threadId, userId);
    tx.db.thread.id.update({
      ...row,
      title: args.clearTitle ? undefined : (args.title ?? row.title),
      systemPromptOverride: args.clearSystemPromptOverride
        ? undefined
        : (args.systemPromptOverride ?? row.systemPromptOverride),
      modelOverride: args.clearModelOverride
        ? undefined
        : (args.modelOverride ?? row.modelOverride),
      metadata: args.clearMetadata
        ? undefined
        : (args.metadata ?? row.metadata),
      updatedAt: tx.timestamp,
    });
  }
);

export const delete_thread = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const userId = requireUserId(ctx);
    const tx = ctx;
    const row = requireOwnedThread(tx, threadId, userId);
    if (tx.db.threadLock.threadId.find(threadId) != null) {
      throwSenderError(`agent.thread_busy:${threadId}`);
    }
    for (const e of [...tx.db.messageEmbedding.threadId.filter(threadId)]) {
      tx.db.messageEmbedding.delete(e);
    }
    for (const a of [...tx.db.messageAttachment.threadId.filter(threadId)]) {
      const blob = tx.db.files.fileBlob.fileId.find(a.fileId);
      if (blob) tx.db.files.fileBlob.delete(blob);
      const file = tx.db.files.file.id.find(a.fileId);
      if (file) tx.db.files.file.delete(file);
      tx.db.messageAttachment.delete(a);
    }
    for (const m of [...tx.db.message.threadId.filter(threadId)]) {
      tx.db.message.delete(m);
    }
    tx.db.thread.delete(row);
  }
);

// Admin-gated; bypasses ownership.
export const clear_thread_lock = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const tx = ctx;
    requireAdmin(tx);
    const lock = tx.db.threadLock.threadId.find(threadId);
    if (lock) tx.db.threadLock.delete(lock);
  }
);

// No-op if the thread already has a title.
export const generate_thread_title = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    const userId = requireUserId(ctx);
    const job = ctx.withTx(tx => {
      const thread = tx.db.thread.id.find(threadId);
      if (!thread) return null;
      if (thread.userId !== userId) {
        throwSenderError(`agent.not_thread_owner:${threadId}`);
      }
      if (thread.title != null && thread.title.length > 0) return null;

      const def = registry.agentDef(thread.agentName);
      if (!def) return null;
      const sumName = def.summarizerAgentName ?? thread.agentName;
      const sumDef = registry.agentDef(sumName);
      if (!sumDef) return null;

      const override = tx.db.agentOverride.agentName.find(sumName);
      const providerName = override?.provider ?? sumDef.defaultProvider;
      const provider = BUILT_IN_PROVIDERS[providerName];
      if (!provider) return null;
      const keyRow = tx.db.apiKey.provider.find(providerName);
      if (!keyRow) return null;

      const msgs = [...tx.db.message.threadId.filter(threadId)];
      msgs.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      const firstUser = msgs.find(m => m.role === 'user');
      if (!firstUser) return null;

      return {
        provider,
        apiKey: keyRow.key,
        model: override?.model ?? sumDef.defaultModel,
        retries: override?.retries ?? sumDef.defaultRetries,
        firstMessage: firstUser.content,
      };
    });
    if (!job) return {};

    const result = callChat(ctx.http, job.provider, {
      apiKey: job.apiKey,
      model: job.model,
      system:
        'You title chat conversations. The user will paste the opening message of ' +
        'a chat. You output a 3-5 word title describing the topic. ' +
        'CRITICAL: do not answer or respond to the message. Do not greet. ' +
        'Output the title and only the title. No quotes, no punctuation at the end.',
      messages: [
        {
          role: 'user',
          content: `Title for a chat that starts with this message:\n\n<message>\n${job.firstMessage}\n</message>`,
        },
      ],
      maxTokens: 30,
      retries: job.retries,
    });
    if (!result.ok || !result.response.text) {
      console.warn(
        `title gen failed: ${result.ok ? 'no text' : result.error.kind}`
      );
      return {};
    }

    const cleaned = result.response.text
      .trim()
      .replace(/^["']|["']$/g, '')
      .replace(/[.!?]+$/g, '')
      .slice(0, 80);

    ctx.withTx(tx => {
      const t2 = tx.db.thread.id.find(threadId);
      if (!t2 || (t2.title != null && t2.title.length > 0)) return;
      tx.db.thread.id.update({
        ...t2,
        title: cleaned,
        updatedAt: tx.timestamp,
      });
    });
    return {};
  }
);

export const request_cancel = spacetimedb.reducer(
  { threadId: t.u64() },
  (ctx, { threadId }) => {
    const userId = requireUserId(ctx);
    const tx = ctx;
    requireOwnedThread(tx, threadId, userId);
    const lock = tx.db.threadLock.threadId.find(threadId);
    if (!lock) throwSenderError(`agent.thread_not_running:${threadId}`);
    if (lock.cancelRequested) return;
    tx.db.threadLock.threadId.update({ ...lock, cancelRequested: true });
  }
);

function resolveProvider(name: string): Provider {
  const p = BUILT_IN_PROVIDERS[name];
  if (!p) throwSenderError(`agent.unknown_provider:${name}`);
  return p;
}

function loadLoopConfigOrThrow(
  tx: WriteCtx,
  threadId: bigint,
  userId: string
): { cfg: LoopConfig; agentName: string; userId: string } {
  const threadRow = requireOwnedThread(tx, threadId, userId);

  const def = registry.agentDef(threadRow.agentName);
  if (!def) {
    throwSenderError(`agent.unknown:${threadRow.agentName}`);
  }

  if (tx.db.threadLock.threadId.find(threadId) != null) {
    throwSenderError(`agent.thread_busy:${threadId}`);
  }
  if (tx.db.agentSecret.singleton.find(true) == null) {
    throwSenderError('agent.not_configured');
  }

  checkRateLimit(tx, userId);

  const override = tx.db.agentOverride.agentName.find(threadRow.agentName);
  const providerName = override?.provider ?? def.defaultProvider;
  const provider = resolveProvider(providerName);

  const keyRow = tx.db.apiKey.provider.find(providerName);
  if (!keyRow) throwSenderError(`agent.no_api_key:${providerName}`);

  return {
    cfg: {
      provider,
      apiKey: keyRow.key,
      model: threadRow.modelOverride ?? override?.model ?? def.defaultModel,
      systemPrompt:
        threadRow.systemPromptOverride ??
        override?.systemPrompt ??
        def.defaultSystemPrompt,
      maxTurns: override?.maxTurns ?? def.defaultMaxTurns,
      maxHistoryMessages:
        override?.maxHistoryMessages ?? def.defaultMaxHistoryMessages,
      maxTokens: override?.maxTokens ?? def.defaultMaxTokens,
      retries: override?.retries ?? def.defaultRetries,
      responseFormat: def.defaultResponseFormat,
    } satisfies LoopConfig,
    agentName: threadRow.agentName,
    userId: threadRow.userId,
  };
}

export const send_message = spacetimedb.procedure(
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
    const content =
      args.content.length > USER_CONTENT_MAX
        ? args.content.slice(0, USER_CONTENT_MAX) + '...[truncated]'
        : args.content;

    const callerUserId = requireUserId(ctx);
    const { cfg, agentName, userId, userMessageId } = ctx.withTx(tx => {
      const loaded = loadLoopConfigOrThrow(tx, args.threadId, callerUserId);
      tx.db.threadLock.insert({
        threadId: args.threadId,
        userId: loaded.userId,
        lockedAt: tx.timestamp,
        cancelRequested: false,
      });
      const inserted = tx.db.message.insert({
        id: 0n,
        threadId: args.threadId,
        userId: loaded.userId,
        role: 'user',
        content,
        toolCallsJson: undefined,
        toolCallId: undefined,
        isError: false,
        promptTokens: undefined,
        completionTokens: undefined,
        createdAt: tx.timestamp,
      });
      for (let i = 0; i < args.attachments.length; i++) {
        const a = args.attachments[i];
        const path = `/msg/${inserted.id}/${i}`;
        const file = tx.db.files.file.insert({
          id: 0n,
          ownerPathKey: files.ownerPathKey(loaded.userId, path),
          path,
          ownerUserId: loaded.userId,
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
          messageId: inserted.id,
          threadId: args.threadId,
          ownerUserId: loaded.userId,
          ordinal: i,
          filename: a.filename,
          createdAt: tx.timestamp,
        });
      }
      const threadRow = tx.db.thread.id.find(args.threadId);
      if (threadRow)
        tx.db.thread.id.update({ ...threadRow, updatedAt: tx.timestamp });
      return { ...loaded, userMessageId: inserted.id };
    });

    maybeEmbedMessage(ctx, args.threadId, userMessageId);
    runLockedLoop(ctx, cfg, agentName, userId, args.threadId, bumpRateLimit);
    return {};
  }
);

export const regenerate_response = spacetimedb.procedure(
  { threadId: t.u64() },
  t.unit(),
  (ctx, { threadId }) => {
    const callerUserId = requireUserId(ctx);
    const { cfg, agentName, userId } = ctx.withTx(tx => {
      const loaded = loadLoopConfigOrThrow(tx, threadId, callerUserId);

      const rows = [...tx.db.message.threadId.filter(threadId)];
      rows.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      let lastUserMsgId: bigint | undefined;
      for (const r of rows) {
        if (r.role === 'user') lastUserMsgId = r.id;
      }
      if (lastUserMsgId === undefined) {
        throwSenderError(`agent.regenerate_no_user_message:${threadId}`);
      }

      for (const r of rows) {
        if (r.id > lastUserMsgId!) tx.db.message.delete(r);
      }

      tx.db.threadLock.insert({
        threadId,
        userId: loaded.userId,
        lockedAt: tx.timestamp,
        cancelRequested: false,
      });
      const threadRow = tx.db.thread.id.find(threadId);
      if (threadRow)
        tx.db.thread.id.update({ ...threadRow, updatedAt: tx.timestamp });
      return loaded;
    });

    runLockedLoop(ctx, cfg, agentName, userId, threadId, bumpRateLimit);
    return {};
  }
);

export const thread_lock_sweep = spacetimedb.reducer(
  { arg: threadLockSweeperTick.rowType },
  (ctx, _arg) => {
    const secret = ctx.db.agentSecret.singleton.find(true);
    const thresholdSecs =
      secret?.staleLockThresholdSecs ?? DEFAULT_STALE_LOCK_THRESHOLD_SECS;
    const thresholdMicros = BigInt(thresholdSecs) * ONE_SECOND_MICROS;

    const cutoffMicros = staleLockCutoffMicros(
      ctx.timestamp.microsSinceUnixEpoch as bigint,
      thresholdMicros
    );
    deleteStaleThreadLocks(
      ctx.db.threadLock.lockedAt.filter(
        new Range(undefined, {
          tag: 'excluded',
          value: new Timestamp(cutoffMicros),
        })
      ),
      cutoffMicros,
      lock => ctx.db.threadLock.delete(lock)
    );
  }
);
