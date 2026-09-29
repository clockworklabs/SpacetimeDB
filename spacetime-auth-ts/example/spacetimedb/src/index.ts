import { schema, t, table, Router, SenderError } from 'spacetimedb/server';
import * as auth from '@spacetimedb/auth/submodule';

const consoleSendMail: auth.SendMailFn = (_ctx, params) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

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
  auth.install(ctx.as.auth);
});

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

export const createNote = spacetimedb.reducer(
  { title: t.string(), body: t.string() },
  (ctx, args) => {
    const userId = auth.requireCallerUserId(ctx.as.auth);
    ctx.db.note.insert({
      noteId: ctx.newUuidV7().toString(),
      authorId: userId,
      title: args.title,
      body: args.body,
      createdAt: ctx.timestamp,
    });
  }
);

export const deleteNote = spacetimedb.reducer(
  { noteId: t.string() },
  (ctx, args) => {
    const userId = auth.requireCallerUserId(ctx.as.auth);
    const row = ctx.db.note.noteId.find(args.noteId);
    if (!row) throw new SenderError('note.not_found');
    if (row.authorId !== userId) throw new SenderError('note.not_owner');
    ctx.db.note.delete(row);
  }
);

export const updateNote = spacetimedb.reducer(
  { noteId: t.string(), title: t.string(), body: t.string() },
  (ctx, args) => {
    const userId = auth.requireCallerUserId(ctx.as.auth);
    const row = ctx.db.note.noteId.find(args.noteId);
    if (!row) throw new SenderError('note.not_found');
    if (row.authorId !== userId) throw new SenderError('note.not_owner');
    ctx.db.note.noteId.update({ ...row, title: args.title, body: args.body });
  }
);

const authHttp = auth.client({
  sendMail: consoleSendMail,
  appName: 'Notes',
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
