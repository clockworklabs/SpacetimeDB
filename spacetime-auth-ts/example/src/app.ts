import {
  authUrlState,
  clearAuthResultParams,
  mountAuthPanel,
} from '@spacetimedb/example-ui';
import '@spacetimedb/example-ui/styles.css';
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
  spacetimeUri: string;
  databaseName: string;
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

function emitAppEvent(name: string, detail: unknown) {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
function emitAuthState() {
  emitAppEvent('auth:state', {
    user: currentUser,
    senderHex: currentSenderHex,
    sessionExpiresAt: currentExp,
  });
}
let lastConnState: string = '';
let lastConnDetail: string = '';
function emitConnectionState(
  state: 'idle' | 'connecting' | 'connected' | 'error',
  detail?: string
) {
  const d = detail ?? '';
  if (state === lastConnState && d === lastConnDetail) return;
  lastConnState = state;
  lastConnDetail = d;
  emitAppEvent('auth:conn', { state, detail });
}
function emitNotes() {
  const sorted = conn
    ? [...conn.db.myNotes.iter()].sort((a, b) =>
        Number(
          b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch
        )
      )
    : [];
  emitAppEvent('auth:notes', { notes: sorted });
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
  authPanel.setProviders({
    google: Boolean(cfg.oauth?.google),
    github: Boolean(cfg.oauth?.github),
  });
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
      .withUri(config.spacetimeUri)
      .withDatabaseName(config.databaseName)
      .withToken(loadStdbToken())
      .onConnect((connection, _identity, token) => {
        if (token) saveStdbToken(token);
        resolve(connection);
      })
      .onDisconnect((_ctx, err) => {
        emitConnectionState('error', err?.message ?? 'disconnected');
        conn = null;
        if (currentUser) scheduleReconnect();
      })
      .onConnectError((_ctx, err) => {
        emitConnectionState('error', 'connect failed');
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
    emitConnectionState('connecting');
    try {
      conn = await connect();
      registerRowCallbacks(conn);
      subscribeToTables(conn);
      emitConnectionState('connected');
    } catch (err) {
      emitConnectionState('error', (err as Error).message);
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
  emitAuthState();
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
  emitAuthState();
}

function subscribeToTables(connection: DbConnection): void {
  connection
    .subscriptionBuilder()
    .onApplied(() => emitNotes())
    .onError((ctx: ErrorContext) => console.error('sub error', ctx.event))
    .subscribe([tables.myNotes, tables.myAuthUser]);
}

function registerRowCallbacks(connection: DbConnection): void {
  connection.db.myNotes.onInsert(() => emitNotes());
  connection.db.myNotes.onUpdate(() => emitNotes());
  connection.db.myNotes.onDelete(() => emitNotes());

  connection.db.myAuthUser.onInsert((_ctx: EventContext, row: AuthUserRow) =>
    syncUserFromRow(row)
  );
  connection.db.myAuthUser.onUpdate(
    (_ctx: EventContext, _o: AuthUserRow, n: AuthUserRow) => syncUserFromRow(n)
  );
  connection.db.myAuthUser.onDelete((_ctx: EventContext, row: AuthUserRow) => {
    if (!currentUser || row.userId !== currentUser.userId) return;
    currentUser = null;
    currentExp = undefined;
    emitAuthState();
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
  emitAuthState();
  emitNotes();
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
  emitAuthState();
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

const authResult = authUrlState(window.location);
const authPanelRoot = document.getElementById('auth-panel');
if (!authPanelRoot) throw new Error('missing_auth_panel');
const authPanel = mountAuthPanel(authPanelRoot, {
  productName: 'Notes',
  actions: {
    login,
    signup,
    forgotPassword,
    resetPassword,
    oauthStart,
  },
  initialMode: authResult.mode,
  resetToken: authResult.resetToken,
});
if (authResult.oauthError) {
  authPanel.showMessage('error', `OAuth: ${authResult.oauthError}`);
}
if (authResult.verified) {
  authPanel.showMessage('success', 'Email verified.');
}
clearAuthResultParams(window.location, window.history);

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

async function main(): Promise<void> {
  emitConnectionState('idle');
  try {
    serverCfg = await loadServerConfig();
  } catch (err) {
    emitConnectionState('error', (err as Error).message);
    return;
  }
  await restoreSession();
  emitAppEvent('auth:ready', {});
}

void main();
