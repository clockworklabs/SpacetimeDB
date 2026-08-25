import { schema, t, table, Router, SenderError } from 'spacetimedb/server';
import type { Timestamp } from 'spacetimedb';
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
  getCallerUserId,
  type SendMailFn,
  type MailParams,
} from '@spacetimedb/auth/submodule';

// Development mailer that logs messages.
const consoleSendMail: SendMailFn = (_ctx, params: MailParams) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

const authUserViewRow = t.object('ExampleAuthUser', {
  userId: t.string(),
  email: t.string(),
  emailVerified: t.bool(),
  name: t.option(t.string()),
  image: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
});

const note = table(
  { name: 'note', public: false },
  {
    noteId: t.string().primaryKey(),
    authorId: t.string().index(),
    title: t.string(),
    body: t.string(),
    createdAt: t.timestamp().index(),
  }
);

const spacetimedb = schema({
  auth,
  note,
});
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  auth.installAuth(ctx.as.auth);
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

export const myNotes = spacetimedb.view(
  { name: 'my_notes', public: true },
  t.array(note.rowType),
  ctx => {
    const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
      ctx.sender
    );
    if (!binding) return [];
    return [...ctx.db.note.authorId.filter(binding.userId)];
  }
);

export const myAuthUser = spacetimedb.view(
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

export const create_note = spacetimedb.reducer(
  { title: t.string(), body: t.string() },
  (ctx, args) => {
    const userId = getCallerUserId(ctx.as.auth);
    if (!userId) throw new SenderError('auth.not_authenticated');
    const noteId = ctx.newUuidV7().toString();
    ctx.db.note.insert({
      noteId,
      authorId: userId,
      title: args.title,
      body: args.body,
      createdAt: ctx.timestamp,
    });
  }
);

export const delete_note = spacetimedb.reducer(
  { noteId: t.string() },
  (ctx, args) => {
    const userId = getCallerUserId(ctx.as.auth);
    if (!userId) throw new SenderError('auth.not_authenticated');
    const row = ctx.db.note.noteId.find(args.noteId);
    if (!row) throw new SenderError('note.not_found');
    if (row.authorId !== userId) throw new SenderError('note.not_owner');
    ctx.db.note.delete(row);
  }
);

export const update_note = spacetimedb.reducer(
  { noteId: t.string(), title: t.string(), body: t.string() },
  (ctx, args) => {
    const userId = getCallerUserId(ctx.as.auth);
    if (!userId) throw new SenderError('auth.not_authenticated');
    const row = ctx.db.note.noteId.find(args.noteId);
    if (!row) throw new SenderError('note.not_found');
    if (row.authorId !== userId) throw new SenderError('note.not_owner');
    ctx.db.note.noteId.update({ ...row, title: args.title, body: args.body });
  }
);

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
  }),
  (ctx, _args) => {
    const userId = getCallerUserId(ctx.as.auth);
    return {
      userId: userId ?? undefined,
      senderIdentityHex: ctx.sender.toHexString(),
    };
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

const forgotHandler = makeForgotPasswordHandler({
  sendMail: consoleSendMail,
  appName: 'Notes',
});
const verifyRequestHandler = makeEmailVerifyRequestHandler({
  sendMail: consoleSendMail,
  appName: 'Notes',
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
);
