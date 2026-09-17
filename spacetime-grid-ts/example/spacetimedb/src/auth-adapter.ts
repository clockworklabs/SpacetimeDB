import { Router, t } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import * as auth from '@spacetimedb/auth/submodule';
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
} from '@spacetimedb/auth/submodule';

import { consoleSendMail, spacetimedb } from './schema';

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

export const linkConnection = spacetimedb.reducer(
  linkConnectionParams,
  (ctx, args) => {
    auth.linkConnection(ctx.as.auth, args);
  }
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
  appName: 'Grid',
});
const verifyRequestHandler = makeEmailVerifyRequestHandler({
  sendMail: consoleSendMail,
  appName: 'Grid',
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
