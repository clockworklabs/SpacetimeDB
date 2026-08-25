import { schema, t, table, Router } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { installAuth } from './install';
import {
  setAuthConfigParams,
  setAuthConfigImpl,
  authSweepImpl,
  getPublicKeyPemParams,
  getPublicKeyPemImpl,
  linkConnectionParams,
  linkConnectionImpl,
  unlinkConnectionParams,
  unlinkConnectionImpl,
  updateProfileParams,
  updateProfileImpl,
  revokeSessionParams,
  revokeSessionImpl,
  listMySessionsParams,
  listMySessionsImpl,
  revokeMySessionParams,
  revokeMySessionImpl,
  passwordSignupHandler,
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
  type SendMailFn,
  type MailParams,
} from '../index';

// Development mailer that logs messages to the SpacetimeDB console.
const consoleSendMail: SendMailFn = (_ctx, params: MailParams) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

// STDB requires submodule tables declared inline at the module's literal site, not imported as values.
const authUser = table(
  { name: 'auth_user', public: false },
  {
    userId: t.string().primaryKey(),
    email: t.string().unique(),
    emailVerified: t.bool(),
    name: t.option(t.string()),
    image: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

const authSession = table(
  { name: 'auth_session', public: false },
  {
    sessionId: t.string().primaryKey(),
    userId: t.string().index(),
    token: t.string().unique(),
    expiresAt: t.timestamp().index(),
    ipAddress: t.option(t.string()),
    userAgent: t.option(t.string()),
    createdAt: t.timestamp(),
  }
);

const authAccount = table(
  { name: 'auth_account', public: false },
  {
    accountId: t.string().primaryKey(),
    userId: t.string().index(),
    providerId: t.string().index(),
    providerAccountId: t.string().index(),
    passwordHash: t.option(t.string()),
    accessToken: t.option(t.string()),
    refreshToken: t.option(t.string()),
    accessTokenExpiresAt: t.option(t.timestamp()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

const authVerification = table(
  { name: 'auth_verification', public: false },
  {
    verificationId: t.string().primaryKey(),
    identifier: t.string().index(),
    value: t.string().unique(),
    purpose: t.string(),
    expiresAt: t.timestamp().index(),
    createdAt: t.timestamp(),
  }
);

const authOauthState = table(
  { name: 'auth_oauth_state', public: false },
  {
    state: t.string().primaryKey(),
    provider: t.string(),
    codeVerifier: t.string(),
    redirectTo: t.string(),
    expiresAt: t.timestamp().index(),
    createdAt: t.timestamp(),
  }
);

const authConfig = table(
  { name: 'auth_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    issuerUrl: t.string(),
    baseUrl: t.string(),
    cookieName: t.string(),
    sessionTtlSeconds: t.u64(),
    es256PrivateKeyPem: t.string(),
    es256PublicKeyPem: t.string(),
    keyId: t.string(),
    googleClientId: t.option(t.string()),
    googleClientSecret: t.option(t.string()),
    githubClientId: t.option(t.string()),
    githubClientSecret: t.option(t.string()),
    updatedAt: t.timestamp(),
  }
);

const authConnectionBinding = table(
  { name: 'auth_connection_binding', public: false },
  {
    stdbIdentity: t.identity().primaryKey(),
    userId: t.string().index(),
    linkedAt: t.timestamp(),
  }
);

const authAdminIdentity = table(
  { name: 'auth_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

const authSweeperTick = table(
  { name: 'auth_sweeper_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({
  rateLimit,
  authUser,
  authSession,
  authAccount,
  authVerification,
  authOauthState,
  authConfig,
  authConnectionBinding,
  authAdminIdentity,
  authSweeperTick,
});
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  installAuth(ctx);
});

// On the first set_auth_config call (no PEM supplied), setAuthConfigImpl generates a fresh ES256 keypair in-module.
export const set_auth_config = spacetimedb.reducer(
  setAuthConfigParams,
  (ctx, args) => {
    setAuthConfigImpl(ctx, args);
  }
);

export const get_auth_public_key = spacetimedb.procedure(
  getPublicKeyPemParams,
  t.object('AuthPubKey', {
    publicKeyPem: t.string(),
    keyId: t.string(),
    issuerUrl: t.string(),
  }),
  getPublicKeyPemImpl
);

export const link_connection = spacetimedb.reducer(
  linkConnectionParams,
  (ctx, args) => {
    linkConnectionImpl(ctx, args);
  }
);

export const unlink_connection = spacetimedb.reducer(
  unlinkConnectionParams,
  (ctx, args) => {
    unlinkConnectionImpl(ctx, args);
  }
);

export const update_profile = spacetimedb.reducer(
  updateProfileParams,
  updateProfileImpl
);

export const revoke_session = spacetimedb.reducer(
  revokeSessionParams,
  (ctx, args) => {
    revokeSessionImpl(ctx, args);
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
  listMySessionsImpl
);

export const revoke_my_session = spacetimedb.reducer(
  revokeMySessionParams,
  (ctx, args) => {
    revokeMySessionImpl(ctx, args);
  }
);

export const auth_sweep = spacetimedb.reducer(
  { onSchedule: authSweeperTick },
  { arg: authSweeperTick.rowType },
  (ctx, _arg) => {
    authSweepImpl(ctx);
  }
);

export const myAuthUser = spacetimedb.view(
  { name: 'my_auth_user', public: true },
  t.array(authUser.rowType),
  ctx => {
    const binding = ctx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) return [];
    const row = ctx.db.authUser.userId.find(binding.userId);
    return row ? [row] : [];
  }
);

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
  }),
  (ctx, _args) => {
    const userId = getCallerUserId(ctx);
    return {
      userId: userId ?? undefined,
      senderIdentityHex: ctx.sender.toHexString(),
    };
  }
);

const localAuthHttp = { secureCookies: false } as const;

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  passwordSignupHandler(ctx, req, localAuthHttp)
);
export const authPasswordLogin = spacetimedb.httpHandler((ctx, req) =>
  passwordLoginHandler(ctx, req, localAuthHttp)
);
export const authMe = spacetimedb.httpHandler(meHandler);
export const authLogout = spacetimedb.httpHandler((ctx, req) =>
  logoutHandler(ctx, req, localAuthHttp)
);
export const authRefresh = spacetimedb.httpHandler((ctx, req) =>
  refreshHandler(ctx, req, localAuthHttp)
);
export const authGoogleStart = spacetimedb.httpHandler((ctx, req) =>
  googleStartHandler(ctx, req, localAuthHttp)
);
export const authGoogleCallback = spacetimedb.httpHandler((ctx, req) =>
  googleCallbackHandler(ctx, req, localAuthHttp)
);
export const authGithubStart = spacetimedb.httpHandler((ctx, req) =>
  githubStartHandler(ctx, req, localAuthHttp)
);
export const authGithubCallback = spacetimedb.httpHandler((ctx, req) =>
  githubCallbackHandler(ctx, req, localAuthHttp)
);

const forgotHandler = makeForgotPasswordHandler({
  sendMail: consoleSendMail,
  appName: 'auth-ts',
});
const verifyRequestHandler = makeEmailVerifyRequestHandler({
  sendMail: consoleSendMail,
  appName: 'auth-ts',
});
const verifyHandler = makeEmailVerifyHandler({
  successRedirect: '/?verified=1',
});

export const authPasswordForgot = spacetimedb.httpHandler(forgotHandler);
export const authPasswordReset = spacetimedb.httpHandler((ctx, req) =>
  resetPasswordHandler(ctx, req, localAuthHttp)
);
export const authEmailVerifyRequest =
  spacetimedb.httpHandler(verifyRequestHandler);
export const authEmailVerify = spacetimedb.httpHandler(verifyHandler);

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
);
