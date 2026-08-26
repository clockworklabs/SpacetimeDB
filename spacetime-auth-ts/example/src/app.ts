import {
  DbConnection,
  tables,
  type EventContext,
  type ErrorContext,
} from './module_bindings/app';

interface AuthUserRow {
  userId: string;
  email: string;
  emailVerified: boolean;
  name?: string;
  image?: string;
  createdAt: unknown;
  updatedAt: unknown;
}

declare global {
  interface Window {
    auth?: {
      signup: (args: {
        email: string;
        password: string;
        name?: string;
      }) => Promise<void>;
      login: (args: { email: string; password: string }) => Promise<void>;
      logout: () => Promise<void>;
      createNote: (args: { title: string; body: string }) => void;
      updateNote: (args: {
        noteId: string;
        title: string;
        body: string;
      }) => void;
      deleteNote: (noteId: string) => void;
      whoami: () => Promise<{
        userId: string | undefined;
        senderIdentityHex: string;
      }>;
      oauthStart: (provider: 'google' | 'github') => void;
      listMySessions: () => Promise<{ sessions: unknown[] }>;
      revokeMySession: (sessionId: string) => void;
      forgotPassword: (email: string) => Promise<void>;
      resetPassword: (token: string, newPassword: string) => Promise<void>;
      requestEmailVerify: () => Promise<void>;
      setProfile: (args: { name?: string; image?: string }) => void;
    };
  }
}

interface AuthMe {
  user: {
    userId: string;
    email: string;
    emailVerified: boolean;
    name?: string;
    image?: string;
  };
  sessionExpiresAt: number;
}

interface ServerConfig {
  stdbUri: string;
  appDatabase: string;
  oauth?: {
    google?: boolean;
    github?: boolean;
  };
}

let conn: DbConnection | null = null;
let serverCfg: ServerConfig | null = null;
let currentUser: AuthMe['user'] | null = null;
let currentExp: number | undefined;
let currentSenderHex: string | undefined;

function dispatch(name: string, detail: unknown) {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
function broadcastAuth() {
  dispatch('auth:state', {
    user: currentUser,
    senderHex: currentSenderHex,
    sessionExpiresAt: currentExp,
  });
}
let lastConnState: string = '';
let lastConnDetail: string = '';
function broadcastConn(
  state: 'idle' | 'connecting' | 'connected' | 'error',
  detail?: string
) {
  const d = detail ?? '';
  if (state === lastConnState && d === lastConnDetail) return;
  lastConnState = state;
  lastConnDetail = d;
  dispatch('auth:conn', { state, detail });
}
function broadcastNotes() {
  const sorted = conn
    ? [...conn.db.myNotes.iter()].sort((a, b) =>
        Number(
          b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch
        )
      )
    : [];
  dispatch('auth:notes', { notes: sorted });
}

async function callJson<T = unknown>(path: string, body?: unknown): Promise<T> {
  const r = await fetch(path, {
    method: body !== undefined ? 'POST' : 'GET',
    headers: body !== undefined ? { 'content-type': 'application/json' } : {},
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: 'same-origin',
  });
  let data: unknown = null;
  try {
    data = await r.json();
  } catch {
    /* empty or non-JSON response */
  }
  if (!r.ok) {
    const error =
      data && typeof data === 'object' && 'error' in data
        ? String((data as { error: unknown }).error)
        : `http_${r.status}`;
    throw new Error(error);
  }
  return data as T;
}

async function loadServerConfig(): Promise<ServerConfig> {
  const r = await fetch('/api/config', { credentials: 'same-origin' });
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  const cfg = (await r.json()) as ServerConfig;
  dispatch('auth:server-config', cfg);
  return cfg;
}

// Persist the STDB identity token so refresh reuses the same identity.
const STDB_TOKEN_KEY = 'notes:stdb_token';
function loadStdbToken(): string | undefined {
  try {
    return localStorage.getItem(STDB_TOKEN_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}
function saveStdbToken(token: string): void {
  try {
    localStorage.setItem(STDB_TOKEN_KEY, token);
  } catch {
    /* Storage can be unavailable. */
  }
}

function connect(): Promise<DbConnection> {
  if (!serverCfg) throw new Error('missing_server_config');
  const config = serverCfg;
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(config.stdbUri)
      .withDatabaseName(config.appDatabase)
      .withToken(loadStdbToken())
      .onConnect((c, _identity, token) => {
        if (token) saveStdbToken(token);
        resolve(c);
      })
      .onDisconnect((_ctx, err) => {
        broadcastConn('error', err?.message ?? 'disconnected');
        conn = null;
        if (currentUser) scheduleReconnect();
      })
      .onConnectError((_ctx, err) => {
        broadcastConn('error', 'connect failed');
        reject(err);
      })
      .build();
  });
}

let reconnectAttempts = 0;
let reconnectTimer: number | null = null;
function scheduleReconnect() {
  if (reconnectTimer != null) return;
  const delay = Math.min(30000, 500 * Math.pow(2, reconnectAttempts));
  reconnectAttempts++;
  reconnectTimer = window.setTimeout(async () => {
    reconnectTimer = null;
    if (!currentUser) return;
    try {
      const r = await callJson<{
        user: AuthMe['user'];
        token: string;
        sessionExpiresAt: number;
      }>('/auth/session/refresh', {});
      await bindSession(r.token, r.user, r.sessionExpiresAt);
      reconnectAttempts = 0;
    } catch {
      scheduleReconnect();
    }
  }, delay);
}

async function bindSession(token: string, user: AuthMe['user'], exp: number) {
  currentUser = user;
  currentExp = exp;

  if (!conn) {
    broadcastConn('connecting');
    try {
      conn = await connect();
      registerRowCallbacks(conn);
      subscribeToTables(conn);
      broadcastConn('connected');
    } catch (err) {
      broadcastConn('error', (err as Error).message);
      return;
    }
  }

  try {
    conn.reducers.linkConnection({ sessionToken: token });
    const w = await conn.procedures.whoami({});
    currentSenderHex = w.senderIdentityHex;
  } catch (err) {
    console.warn('link_connection failed', err);
  }
  broadcastAuth();
}

function syncUserFromRow(row: AuthUserRow) {
  if (!currentUser || row.userId !== currentUser.userId) return;
  currentUser = {
    userId: row.userId,
    email: row.email,
    emailVerified: row.emailVerified,
    name: row.name ?? undefined,
    image: row.image ?? undefined,
  };
  broadcastAuth();
}

function subscribeToTables(c: DbConnection): void {
  c.subscriptionBuilder()
    .onApplied(() => broadcastNotes())
    .onError((ctx: ErrorContext) => console.error('sub error', ctx.event))
    .subscribe([tables.myNotes, tables.myAuthUser]);
}

function registerRowCallbacks(c: DbConnection): void {
  c.db.myNotes.onInsert(() => broadcastNotes());
  c.db.myNotes.onUpdate(() => broadcastNotes());
  c.db.myNotes.onDelete(() => broadcastNotes());

  c.db.myAuthUser.onInsert((_ctx: EventContext, row: AuthUserRow) =>
    syncUserFromRow(row)
  );
  c.db.myAuthUser.onUpdate(
    (_ctx: EventContext, _o: AuthUserRow, n: AuthUserRow) => syncUserFromRow(n)
  );
  c.db.myAuthUser.onDelete((_ctx: EventContext, row: AuthUserRow) => {
    if (!currentUser || row.userId !== currentUser.userId) return;
    currentUser = null;
    currentExp = undefined;
    broadcastAuth();
  });
}

async function signup(args: {
  email: string;
  password: string;
  name?: string;
}) {
  const r = await callJson<{ token: string }>('/auth/password/signup', args);
  const me = await callJson<AuthMe>('/auth/me');
  await bindSession(r.token, me.user, me.sessionExpiresAt);
}

async function login(args: { email: string; password: string }) {
  const r = await callJson<{ token: string }>('/auth/password/login', args);
  const me = await callJson<AuthMe>('/auth/me');
  await bindSession(r.token, me.user, me.sessionExpiresAt);
}

async function restoreSession(): Promise<boolean> {
  try {
    const r = await callJson<{
      user: AuthMe['user'];
      token: string;
      sessionExpiresAt: number;
    }>('/auth/session/refresh', {});
    await bindSession(r.token, r.user, r.sessionExpiresAt);
    return true;
  } catch {
    return false;
  }
}

async function logout() {
  if (conn) {
    try {
      conn.reducers.unlinkConnection({});
    } catch {
      /* best-effort disconnect cleanup */
    }
  }
  await callJson('/auth/logout', {});
  currentUser = null;
  currentExp = undefined;
  currentSenderHex = undefined;
  broadcastAuth();
  broadcastNotes();
}

function createNote(args: { title: string; body: string }) {
  if (!conn) throw new Error('not_connected');
  conn.reducers.createNote(args);
}

function deleteNote(noteId: string) {
  if (!conn) throw new Error('not_connected');
  conn.reducers.deleteNote({ noteId });
}

function updateNote(args: { noteId: string; title: string; body: string }) {
  if (!conn) throw new Error('not_connected');
  conn.reducers.updateNote(args);
}

async function whoami() {
  if (!conn) throw new Error('not_connected');
  const r = await conn.procedures.whoami({});
  currentSenderHex = r.senderIdentityHex;
  broadcastAuth();
  return r;
}

async function listMySessions() {
  if (!conn) throw new Error('not_connected');
  return await conn.procedures.listMySessions({});
}

function revokeMySession(sessionId: string) {
  if (!conn) throw new Error('not_connected');
  conn.reducers.revokeMySession({ sessionId });
}

async function forgotPassword(email: string) {
  await callJson('/auth/password/forgot', { email });
}

async function resetPassword(token: string, newPassword: string) {
  await callJson('/auth/password/reset', { token, newPassword });
}

async function requestEmailVerify() {
  await callJson('/auth/email/verify-request', {});
}

function oauthStart(provider: 'google' | 'github') {
  window.location.href = `/auth/${provider}/start?redirectTo=/`;
}

function setProfile(args: { name?: string; image?: string }) {
  if (!conn) throw new Error('not_connected');
  conn.reducers.updateProfile({ name: args.name, image: args.image });
}

window.auth = {
  signup,
  login,
  logout,
  createNote,
  updateNote,
  deleteNote,
  whoami,
  oauthStart,
  listMySessions,
  revokeMySession,
  forgotPassword,
  resetPassword,
  requestEmailVerify,
  setProfile,
};

(async () => {
  broadcastConn('idle');
  try {
    serverCfg = await loadServerConfig();
  } catch (err) {
    broadcastConn('error', (err as Error).message);
    return;
  }
  await restoreSession();
  dispatch('auth:ready', {});
})();
