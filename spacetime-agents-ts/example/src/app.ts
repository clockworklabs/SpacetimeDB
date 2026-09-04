import {
  authUrlState,
  clearAuthResultParams,
  mountAuthPanel,
} from '@spacetimedb/submodule-shared';
import '@spacetimedb/submodule-shared/styles.css';
import {
  DbConnection,
  tables,
  type ErrorContext,
  type EventContext,
  type SubscriptionHandle,
} from './module_bindings/app';
import type {
  File as FileRow,
  AgentAuthUser as AuthUserRow,
  AgentConfigStatus,
  MySessions,
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
interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
  oauth?: {
    google?: boolean;
    github?: boolean;
  };
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
      listMySessions: () => Promise<MySessions>;
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
let serverCfg: ServerConfig | null = null;

let currentUser: AuthUser | null = null;
let currentExp: number | undefined;

let reconnectAttempt = 0;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000, 15000];

function emitAppEvent(name: string, detail: unknown): void {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
function emitThreads(): void {
  if (!currentConn) {
    emitAppEvent('stdb:threads', { threads: [] });
    return;
  }
  const sorted = [...currentConn.db.myThreads.iter()].sort((a, b) => {
    const av = a.updatedAt.microsSinceUnixEpoch as bigint;
    const bv = b.updatedAt.microsSinceUnixEpoch as bigint;
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
  emitAppEvent('stdb:threads', { threads: sorted });
}
function emitMessages(): void {
  if (!currentConn) {
    emitAppEvent('stdb:messages', { messages: [], attachments: {} });
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
  emitAppEvent('stdb:messages', { messages: sorted, attachments: atts });
}
function emitThreadLocks(): void {
  if (!currentConn) {
    emitAppEvent('stdb:locks', { locks: [] });
    return;
  }
  const entries: Array<[bigint, boolean]> = [];
  for (const l of currentConn.db.myThreadLocks.iter())
    entries.push([l.threadId, l.cancelRequested]);
  emitAppEvent('stdb:locks', { locks: entries });
}
function emitAgentOverrides(): void {
  if (!currentConn) {
    emitAppEvent('stdb:overrides', { overrides: [] });
    return;
  }
  emitAppEvent('stdb:overrides', {
    overrides: [...currentConn.db.agentOverride.iter()],
  });
}
function emitConfigState(): void {
  emitAppEvent('stdb:config', { state: configState });
}
function emitConnectionState(state: ConnState, detail?: string): void {
  emitAppEvent('stdb:connState', { state, detail });
}
function emitAuthState(): void {
  emitAppEvent('auth:state', {
    user: currentUser,
    sessionExpiresAt: currentExp,
  });
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
  emitAuthState();
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

async function loadServerConfig(): Promise<ServerConfig> {
  const res = await fetch('/api/config', { credentials: 'same-origin' });
  if (!res.ok) throw new Error(`/api/config returned ${res.status}`);
  const nextConfig = (await res.json()) as ServerConfig;
  authPanel.setProviders({
    google: Boolean(nextConfig.oauth?.google),
    github: Boolean(nextConfig.oauth?.github),
  });
  return nextConfig;
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
        emitConnectionState('error', err?.message ?? 'disconnected');
        currentConn = null;
        globalSub = null;
        messageSub = null;
        if (currentUser) scheduleReconnect();
      })
      .onConnectError((_ctx, err) => {
        emitConnectionState('error', err?.message ?? 'connect failed');
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
  emitMessages();

  if (threadId === null || !currentConn) return;

  messageSub = currentConn
    .subscriptionBuilder()
    .onApplied(() => emitMessages())
    .onError((ctx: ErrorContext) =>
      console.error('message sub error', ctx.event)
    )
    .subscribe([tables.myMessages.where(row => row.threadId.eq(threadId))]);
}

function registerRowCallbacks(connection: DbConnection): void {
  connection.db.myThreads.onInsert(() => emitThreads());
  connection.db.myThreads.onUpdate(() => emitThreads());
  connection.db.myThreads.onDelete(() => emitThreads());

  connection.db.myMessages.onInsert(() => emitMessages());
  connection.db.myMessages.onUpdate(() => emitMessages());
  connection.db.myMessages.onDelete(() => emitMessages());

  connection.db.myFiles.onInsert(() => emitMessages());
  connection.db.myFiles.onUpdate(() => emitMessages());
  connection.db.myFiles.onDelete(() => emitMessages());

  connection.db.myThreadLocks.onInsert(() => emitThreadLocks());
  connection.db.myThreadLocks.onUpdate(() => emitThreadLocks());
  connection.db.myThreadLocks.onDelete(() => emitThreadLocks());

  connection.db.agentOverride.onInsert(() => emitAgentOverrides());
  connection.db.agentOverride.onUpdate(() => emitAgentOverrides());
  connection.db.agentOverride.onDelete(() => emitAgentOverrides());

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

function subscribeToTables(connection: DbConnection): SubscriptionHandle {
  return connection
    .subscriptionBuilder()
    .onApplied(() => {
      emitThreads();
      emitThreadLocks();
      emitAgentOverrides();
      emitMessages();
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
  emitConfigState();
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
    emitConnectionState('connecting');
    try {
      const conn = await connect(
        serverCfg.spacetimeUri,
        serverCfg.databaseName
      );
      currentConn = conn;
      reconnectAttempt = 0;
      emitConnectionState('connected');

      emitThreads();
      emitMessages();
      emitThreadLocks();
      emitAgentOverrides();

      registerRowCallbacks(conn);
    } catch (err) {
      emitConnectionState(
        'error',
        err instanceof Error ? err.message : String(err)
      );
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
  emitAuthState();
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
  emitThreads();
  emitMessages();
  emitThreadLocks();
  emitAgentOverrides();
  emitAuthState();
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

async function listMySessions(): Promise<MySessions> {
  return await requireConn().procedures.listMySessions({});
}
async function revokeMySession(sessionId: string): Promise<void> {
  requireConn().reducers.revokeMySession({ sessionId });
}

const authResult = authUrlState(window.location);
const authPanelRoot = document.getElementById('auth-panel');
if (!authPanelRoot) throw new Error('missing_auth_panel');
const authPanel = mountAuthPanel(authPanelRoot, {
  productName: 'Agents',
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

  emitConnectionState('idle');
  serverCfg = await loadServerConfig();
  emitAppEvent('stdb:ready', {});
  await restoreSession();
  emitAppEvent('auth:ready', {});
}

main().catch(err => {
  console.error(err);
  emitConnectionState(
    'error',
    err instanceof Error ? err.message : String(err)
  );
});
