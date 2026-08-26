import {
  DbConnection,
  tables,
  type ErrorContext,
  type EventContext,
  type SubscriptionHandle,
} from './module_bindings/app';
import type {
  File as FileRow,
  AgentConfigStatus,
} from './module_bindings/app/types';

interface AuthUser {
  userId: string;
  email: string;
  emailVerified: boolean;
  name?: string;
  image?: string;
}
interface AuthMe {
  user: AuthUser;
  sessionExpiresAt: number;
}
interface AuthUserRow extends AuthUser {
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
      oauthStart: (provider: 'google' | 'github') => void;
      forgotPassword: (email: string) => Promise<void>;
      resetPassword: (token: string, newPassword: string) => Promise<void>;
      requestEmailVerify: () => Promise<void>;
      listMySessions: () => Promise<{ sessions: unknown[] }>;
      revokeMySession: (sessionId: string) => Promise<void>;
      setProfile: (args: { name?: string; image?: string }) => void;
    };
    stdb?: {
      setAgentSecret: (args: {
        staleLockThresholdSecs: number | undefined;
        rateLimitTokensPerWindow: number | undefined;
        rateLimitWindowSecs: number | undefined;
      }) => Promise<void>;
      setApiKey: (provider: string, key: string) => Promise<void>;
      clearApiKey: (provider: string) => Promise<void>;
      setAgentOverride: (args: {
        agentName: string;
        provider: string | undefined;
        model: string | undefined;
        systemPrompt: string | undefined;
        maxTurns: number | undefined;
        maxHistoryMessages: number | undefined;
        maxTokens: number | undefined;
        retries: number | undefined;
      }) => Promise<void>;
      clearAgentOverride: (agentName: string) => Promise<void>;
      getAgentConfigStatus: () => Promise<AgentConfigStatus>;
      setActiveThread: (threadId: bigint | null) => void;
      startThread: (args: {
        agentName: string;
        title: string | undefined;
        systemPromptOverride: string | undefined;
        metadata: string | undefined;
      }) => Promise<bigint>;
      updateThread: (args: {
        threadId: bigint;
        title: string | undefined;
        systemPromptOverride: string | undefined;
        modelOverride: string | undefined;
        metadata: string | undefined;
        clearTitle: boolean;
        clearSystemPromptOverride: boolean;
        clearModelOverride: boolean;
        clearMetadata: boolean;
      }) => Promise<void>;
      deleteThread: (threadId: bigint) => Promise<void>;
      sendMessage: (
        threadId: bigint,
        content: string,
        attachments?: Array<{
          mimeType: string;
          filename: string | undefined;
          bytes: Uint8Array;
        }>
      ) => Promise<void>;
      regenerateResponse: (threadId: bigint) => Promise<void>;
      requestCancel: (threadId: bigint) => Promise<void>;
      generateThreadTitle: (threadId: bigint) => Promise<void>;
      clearThreadLock: (threadId: bigint) => Promise<void>;
    };
  }
}

type ConfigState =
  | { kind: 'unknown' }
  | { kind: 'unconfigured' }
  | { kind: 'configured'; status: AgentConfigStatus };
type ConnState = 'idle' | 'connecting' | 'connected' | 'error';

let configState: ConfigState = { kind: 'unknown' };

let currentConn: DbConnection | null = null;
let globalSub: SubscriptionHandle | null = null;
let messageSub: SubscriptionHandle | null = null;
let activeThreadId: bigint | null = null;
let serverCfg: { spacetimeUri: string; databaseName: string } | null = null;

let currentUser: AuthUser | null = null;
let currentExp: number | undefined;

let reconnectAttempt = 0;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000, 15000];

function dispatch(name: string, detail: unknown): void {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
function broadcastThreads(): void {
  if (!currentConn) {
    dispatch('stdb:threads', { threads: [] });
    return;
  }
  const sorted = [...currentConn.db.myThreads.iter()].sort((a, b) => {
    const av = a.updatedAt.microsSinceUnixEpoch as bigint;
    const bv = b.updatedAt.microsSinceUnixEpoch as bigint;
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
  dispatch('stdb:threads', { threads: sorted });
}
function broadcastMessages(): void {
  if (!currentConn) {
    dispatch('stdb:messages', { messages: [], attachments: {} });
    return;
  }
  const sorted = [...currentConn.db.myMessages.iter()].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  );
  const atts: Record<string, FileRow[]> = {};
  for (const f of currentConn.db.myFiles.iter()) {
    if (f.messageId === undefined) continue;
    const key = f.messageId.toString();
    (atts[key] ??= []).push(f);
  }
  dispatch('stdb:messages', { messages: sorted, attachments: atts });
}
function broadcastLocks(): void {
  if (!currentConn) {
    dispatch('stdb:locks', { locks: [] });
    return;
  }
  const entries: Array<[bigint, boolean]> = [];
  for (const l of currentConn.db.myThreadLocks.iter())
    entries.push([l.threadId, l.cancelRequested]);
  dispatch('stdb:locks', { locks: entries });
}
function broadcastOverrides(): void {
  if (!currentConn) {
    dispatch('stdb:overrides', { overrides: [] });
    return;
  }
  dispatch('stdb:overrides', {
    overrides: [...currentConn.db.agentOverride.iter()],
  });
}
function broadcastConfig(): void {
  dispatch('stdb:config', { state: configState });
}
function broadcastConn(state: ConnState, detail?: string): void {
  dispatch('stdb:connState', { state, detail });
}
function broadcastAuth(): void {
  dispatch('auth:state', { user: currentUser, sessionExpiresAt: currentExp });
}

function syncUserFromRow(row: AuthUserRow): void {
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

function requireConn(): DbConnection {
  if (!currentConn) throw new Error('STDB not connected');
  return currentConn;
}

// Authentication requests proxied to SpacetimeDB by the Express server
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
    /* empty body */
  }
  if (!r.ok) {
    const err =
      data && typeof data === 'object' && 'error' in data
        ? String((data as { error: unknown }).error)
        : `http_${r.status}`;
    throw new Error(err);
  }
  return data as T;
}

async function loadServerConfig(): Promise<{
  spacetimeUri: string;
  databaseName: string;
}> {
  const res = await fetch('/api/config', { credentials: 'same-origin' });
  if (!res.ok) throw new Error(`/api/config returned ${res.status}`);
  return res.json();
}

// Persist the STDB identity token so refresh reuses the same identity.
const STDB_TOKEN_KEY = 'agents:stdb_token';
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

function connect(uri: string, databaseName: string): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(uri)
      .withDatabaseName(databaseName)
      .withToken(loadStdbToken())
      .onConnect((connection, _identity, token) => {
        if (token) saveStdbToken(token);
        resolve(connection);
      })
      .onDisconnect((_ctx, err) => {
        broadcastConn('error', err?.message ?? 'disconnected');
        currentConn = null;
        globalSub = null;
        messageSub = null;
        if (currentUser) scheduleReconnect();
      })
      .onConnectError((_ctx, err) => {
        broadcastConn('error', err?.message ?? 'connect failed');
        reject(err);
      })
      .build();
  });
}

function scheduleReconnect(): void {
  if (reconnectTimer) return;
  const delay =
    RECONNECT_DELAYS_MS[
      Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)
    ];
  console.warn(
    `STDB reconnect in ${delay}ms (attempt ${reconnectAttempt + 1})`
  );
  reconnectTimer = setTimeout(async () => {
    reconnectTimer = null;
    reconnectAttempt++;
    if (!currentUser) return;
    try {
      const r = await callJson<{
        user: AuthUser;
        token: string;
        sessionExpiresAt: number;
      }>('/auth/session/refresh', {});
      await bindSession(r.token, r.user, r.sessionExpiresAt);
      reconnectAttempt = 0;
    } catch (err) {
      console.error('Reconnect failed:', err);
      scheduleReconnect();
    }
  }, delay);
}

function setActiveThread(threadId: bigint | null): void {
  if (activeThreadId === threadId) return;
  activeThreadId = threadId;

  if (messageSub) {
    messageSub.unsubscribe();
    messageSub = null;
  }
  broadcastMessages();

  if (threadId === null || !currentConn) return;

  messageSub = currentConn
    .subscriptionBuilder()
    .onApplied(() => broadcastMessages())
    .onError((ctx: ErrorContext) =>
      console.error('message sub error', ctx.event)
    )
    .subscribe([tables.myMessages.where(row => row.threadId.eq(threadId))]);
}

function registerRowCallbacks(connection: DbConnection): void {
  connection.db.myThreads.onInsert(() => broadcastThreads());
  connection.db.myThreads.onUpdate(() => broadcastThreads());
  connection.db.myThreads.onDelete(() => broadcastThreads());

  connection.db.myMessages.onInsert(() => broadcastMessages());
  connection.db.myMessages.onUpdate(() => broadcastMessages());
  connection.db.myMessages.onDelete(() => broadcastMessages());

  connection.db.myFiles.onInsert(() => broadcastMessages());
  connection.db.myFiles.onUpdate(() => broadcastMessages());
  connection.db.myFiles.onDelete(() => broadcastMessages());

  connection.db.myThreadLocks.onInsert(() => broadcastLocks());
  connection.db.myThreadLocks.onUpdate(() => broadcastLocks());
  connection.db.myThreadLocks.onDelete(() => broadcastLocks());

  connection.db.agentOverride.onInsert(() => broadcastOverrides());
  connection.db.agentOverride.onUpdate(() => broadcastOverrides());
  connection.db.agentOverride.onDelete(() => broadcastOverrides());

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
    broadcastAuth();
  });
}

function subscribeToTables(connection: DbConnection): SubscriptionHandle {
  return connection
    .subscriptionBuilder()
    .onApplied(() => {
      broadcastThreads();
      broadcastLocks();
      broadcastOverrides();
      broadcastMessages();
    })
    .onError((ctx: ErrorContext) =>
      console.error('global sub error', ctx.event)
    )
    .subscribe([
      tables.myThreads,
      tables.myThreadLocks,
      tables.agentOverride,
      tables.myFiles,
      tables.myAuthUser,
    ]);
}

async function refreshConfigStatus(): Promise<AgentConfigStatus> {
  const status = await requireConn().procedures.getAgentConfigStatus({});
  configState = status.isConfigured
    ? { kind: 'configured', status }
    : { kind: 'unconfigured' };
  broadcastConfig();
  return status;
}

async function bindSession(
  token: string,
  user: AuthUser,
  exp: number
): Promise<void> {
  currentUser = user;
  currentExp = exp;

  if (!serverCfg) serverCfg = await loadServerConfig();

  if (!currentConn) {
    broadcastConn('connecting');
    try {
      const conn = await connect(
        serverCfg.spacetimeUri,
        serverCfg.databaseName
      );
      currentConn = conn;
      reconnectAttempt = 0;
      broadcastConn('connected');

      broadcastThreads();
      broadcastMessages();
      broadcastLocks();
      broadcastOverrides();

      registerRowCallbacks(conn);
    } catch (err) {
      broadcastConn('error', err instanceof Error ? err.message : String(err));
      return;
    }
  }

  // Link the connection before subscribing because views read the binding.
  try {
    await currentConn.procedures.linkConnection({ sessionToken: token });
  } catch (err) {
    console.warn('link_connection failed', err);
  }

  if (!globalSub) {
    globalSub = subscribeToTables(currentConn);

    const previousActive = activeThreadId;
    activeThreadId = null;
    messageSub = null;
    if (previousActive !== null) setActiveThread(previousActive);
  }

  await refreshConfigStatus();
  broadcastAuth();
}

async function restoreSession(): Promise<boolean> {
  try {
    const r = await callJson<{
      user: AuthUser;
      token: string;
      sessionExpiresAt: number;
    }>('/auth/session/refresh', {});
    await bindSession(r.token, r.user, r.sessionExpiresAt);
    return true;
  } catch {
    return false;
  }
}

async function signup(args: {
  email: string;
  password: string;
  name?: string;
}): Promise<void> {
  const r = await callJson<{ token: string }>('/auth/password/signup', args);
  const me = await callJson<AuthMe>('/auth/me');
  await bindSession(r.token, me.user, me.sessionExpiresAt);
}

async function login(args: { email: string; password: string }): Promise<void> {
  const r = await callJson<{ token: string }>('/auth/password/login', args);
  const me = await callJson<AuthMe>('/auth/me');
  await bindSession(r.token, me.user, me.sessionExpiresAt);
}

async function logout(): Promise<void> {
  if (currentConn) {
    try {
      currentConn.reducers.unlinkConnection({});
    } catch {
      /* ignore */
    }
  }
  try {
    await callJson('/auth/logout', {});
  } catch {
    /* ignore */
  }
  currentUser = null;
  currentExp = undefined;
  broadcastThreads();
  broadcastMessages();
  broadcastLocks();
  broadcastOverrides();
  broadcastAuth();
}

function oauthStart(provider: 'google' | 'github'): void {
  window.location.href = `/auth/${provider}/start?redirectTo=/`;
}

async function forgotPassword(email: string): Promise<void> {
  await callJson('/auth/password/forgot', { email });
}
async function resetPassword(
  token: string,
  newPassword: string
): Promise<void> {
  await callJson('/auth/password/reset', { token, newPassword });
}
async function requestEmailVerify(): Promise<void> {
  await callJson('/auth/email/verify-request', {});
}

async function listMySessions(): Promise<{ sessions: unknown[] }> {
  return await requireConn().procedures.listMySessions({});
}
async function revokeMySession(sessionId: string): Promise<void> {
  requireConn().reducers.revokeMySession({ sessionId });
}

async function main(): Promise<void> {
  window.auth = {
    signup,
    login,
    logout,
    oauthStart,
    forgotPassword,
    resetPassword,
    requestEmailVerify,
    listMySessions,
    revokeMySession,
    setProfile: args => {
      requireConn().reducers.updateProfile({
        name: args.name,
        image: args.image,
      });
    },
  };

  window.stdb = {
    setAgentSecret: async args => {
      requireConn().reducers.setAgentSecret(args);
      await refreshConfigStatus();
    },
    setApiKey: async (provider, key) => {
      requireConn().reducers.setApiKey({ provider, key });
      await refreshConfigStatus();
    },
    clearApiKey: async provider => {
      requireConn().reducers.clearApiKey({ provider });
      await refreshConfigStatus();
    },
    setAgentOverride: async args => {
      requireConn().reducers.setAgentOverride(args);
    },
    clearAgentOverride: async agentName => {
      requireConn().reducers.clearAgentOverride({ agentName });
    },
    getAgentConfigStatus: () => refreshConfigStatus(),
    setActiveThread,
    startThread: async args => {
      return await requireConn().procedures.startThread(args);
    },
    updateThread: async args => {
      requireConn().reducers.updateThread(args);
    },
    deleteThread: async threadId => {
      requireConn().reducers.deleteThread({ threadId });
    },
    sendMessage: async (threadId, content, atts) => {
      await requireConn().procedures.sendMessage({
        threadId,
        content,
        attachments: atts ?? [],
      });
    },
    regenerateResponse: async threadId => {
      await requireConn().procedures.regenerateResponse({ threadId });
    },
    requestCancel: async threadId => {
      requireConn().reducers.requestCancel({ threadId });
    },
    generateThreadTitle: async threadId => {
      await requireConn().procedures.generateThreadTitle({ threadId });
    },
    clearThreadLock: async threadId => {
      requireConn().reducers.clearThreadLock({ threadId });
    },
  };

  broadcastConn('idle');
  dispatch('stdb:ready', {});
  await restoreSession();
  dispatch('auth:ready', {});
}

main().catch(err => {
  console.error(err);
  broadcastConn('error', err instanceof Error ? err.message : String(err));
});
