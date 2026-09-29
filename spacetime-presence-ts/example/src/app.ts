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
} from './module_bindings/app';
import type { AuthUser as AuthUserRow } from './module_bindings/app/auth/types';
import type {
  Server,
  ChatRateLimitStatus,
  MessageThread,
  ThreadMessage,
} from './module_bindings/app/types';

interface AttachmentInput {
  mimeType: string;
  filename: string | undefined;
  bytes: Uint8Array;
}

declare global {
  interface Window {
    chat?: {
      setDisplayName: (displayName: string) => Promise<void>;
      setStatus: (
        status: 'online' | 'away' | 'dnd' | 'invisible'
      ) => Promise<void>;
      createServer: (name: string) => Promise<void>;
      renameServer: (serverId: bigint, name: string) => Promise<void>;
      deleteServer: (serverId: bigint) => Promise<void>;
      joinServer: (serverId: bigint) => Promise<void>;
      leaveServer: (serverId: bigint) => Promise<void>;
      setActiveServer: (serverId: bigint | null) => void;
      createRoom: (
        serverId: bigint,
        name: string,
        isPrivate: boolean,
        category?: string
      ) => Promise<void>;
      joinRoom: (roomId: bigint) => Promise<void>;
      addRoomMember: (roomId: bigint, userId: string) => Promise<void>;
      leaveRoom: (roomId: bigint) => Promise<void>;
      sendMessage: (
        roomId: bigint,
        content: string,
        replyToMessageId?: bigint,
        attachments?: AttachmentInput[]
      ) => Promise<void>;
      sendThreadMessage: (
        rootMessageId: bigint,
        content: string
      ) => Promise<void>;
      getAttachmentFile: (
        fileId: bigint
      ) => Promise<{ filename?: string; mimeType: string; bytes: Uint8Array }>;
      editMessage: (messageId: bigint, content: string) => Promise<void>;
      deleteMessage: (messageId: bigint) => Promise<void>;
      editThreadMessage: (
        threadMessageId: bigint,
        content: string
      ) => Promise<void>;
      deleteThreadMessage: (threadMessageId: bigint) => Promise<void>;
      renameRoom: (roomId: bigint, name: string) => Promise<void>;
      setRoomCategory: (roomId: bigint, category?: string) => Promise<void>;
      setRoomPrivacy: (roomId: bigint, isPrivate: boolean) => Promise<void>;
      deleteRoom: (roomId: bigint) => Promise<void>;
      startTyping: (roomId: bigint) => Promise<void>;
      stopTyping: (roomId: bigint) => Promise<void>;
      markRoomRead: (roomId: bigint) => Promise<void>;
      toggleReaction: (messageId: bigint, emoji: string) => Promise<void>;
      pinMessage: (messageId: bigint) => Promise<void>;
      unpinMessage: (messageId: bigint) => Promise<void>;
      searchMessages: (roomId: bigint, query: string) => Promise<unknown[]>;
      setActiveRoom: (roomId: bigint | null) => void;
      heartbeat: () => Promise<void>;
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
      setProfile: (args: { name?: string; image?: string }) => Promise<void>;
    };
  }
}

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
  oauth?: {
    google?: boolean;
    github?: boolean;
  };
}

interface AuthUser {
  userId: string;
  email: string;
  emailVerified: boolean;
  name?: string;
  image?: string;
}

interface AuthRefreshResponse {
  user: AuthUser;
  token: string;
  sessionExpiresAt: number;
}

interface AuthMeResponse {
  user: AuthUser;
  sessionExpiresAt: number;
}

const HEARTBEAT_INTERVAL_MS = 15_000;
const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000, 15000];

let config: ServerConfig | null = null;
let conn: DbConnection | null = null;
let activeServerId: bigint | null = null;
let activeRoomId: bigint | null = null;
let heartbeatTimer: ReturnType<typeof setInterval> | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectAttempt = 0;
let authUser: AuthUser | null = null;
let sessionExpiresAt: number | undefined;

function normalizeError(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

function emitConnectionState(
  state: 'connecting' | 'connected' | 'error',
  detail?: string
): void {
  window.dispatchEvent(
    new CustomEvent('chat:conn', { detail: { state, detail } })
  );
}

function emitAuthState(): void {
  window.dispatchEvent(
    new CustomEvent('chat:auth', {
      detail: {
        user: authUser,
        sessionExpiresAt,
      },
    })
  );
}

function emitPresenceState(): void {
  if (!conn) {
    window.dispatchEvent(
      new CustomEvent('chat:data', {
        detail: {
          activeServerId,
          activeRoomId,
          servers: [],
          directory: [],
          serverMembers: [],
          rooms: [],
          users: [],
          members: [],
          messages: [],
          reactions: [],
          attachments: [],
          threads: [],
          threadMessages: [],
          cursors: [],
          presence: [],
          rateLimitStatus: [],
          authenticated: Boolean(authUser),
        },
      })
    );
    return;
  }
  const c = conn;
  const serverRows = [...c.db.myServers.iter()].sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch as bigint;
    const bv = b.createdAt.microsSinceUnixEpoch as bigint;
    return av < bv ? -1 : av > bv ? 1 : 0;
  });
  const roomRows = [...c.db.myRooms.iter()].sort((a, b) => {
    if (a.name === 'general') return -1;
    if (b.name === 'general') return 1;
    return a.name.localeCompare(b.name);
  });
  const userRows = [...c.db.myChatUsers.iter()].sort((a, b) =>
    a.displayName.localeCompare(b.displayName)
  );
  const messageRows = [...c.db.myRoomMessages.iter()].sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch as bigint;
    const bv = b.createdAt.microsSinceUnixEpoch as bigint;
    return av < bv ? -1 : av > bv ? 1 : 0;
  });
  const threadRows = [...c.db.myMessageThreads.iter()].sort(
    (a: MessageThread, b: MessageThread) => {
      const av = a.updatedAt.microsSinceUnixEpoch as bigint;
      const bv = b.updatedAt.microsSinceUnixEpoch as bigint;
      return av < bv ? 1 : av > bv ? -1 : 0;
    }
  );
  const threadMessageRows = [...c.db.myThreadMessages.iter()].sort(
    (a: ThreadMessage, b: ThreadMessage) => {
      const av = a.createdAt.microsSinceUnixEpoch as bigint;
      const bv = b.createdAt.microsSinceUnixEpoch as bigint;
      return av < bv ? -1 : av > bv ? 1 : 0;
    }
  );
  window.dispatchEvent(
    new CustomEvent('chat:data', {
      detail: {
        activeServerId,
        activeRoomId,
        servers: serverRows,
        directory: [...c.db.serverDirectory.iter()],
        serverMembers: [...c.db.myServerMembers.iter()],
        rooms: roomRows,
        users: userRows,
        members: [...c.db.myRoomMembers.iter()],
        messages: messageRows,
        reactions: [...c.db.myRoomMessageReactions.iter()],
        attachments: [...c.db.myRoomAttachments.iter()].sort(
          (a, b) => a.ordinal - b.ordinal
        ),
        threads: threadRows,
        threadMessages: threadMessageRows,
        cursors: [...c.db.myRoomReadCursors.iter()],
        presence: [...c.db.myPresenceEntries.iter()],
        rateLimitStatus: [
          ...c.db.myRateLimitStatus.iter(),
        ] as ChatRateLimitStatus[],
        authenticated: Boolean(authUser),
      },
    })
  );
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
  const r = await fetch('/api/config');
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  const nextConfig = (await r.json()) as ServerConfig;
  authPanel.setProviders({
    google: Boolean(nextConfig.oauth?.google),
    github: Boolean(nextConfig.oauth?.github),
  });
  return nextConfig;
}

async function signup(args: {
  email: string;
  password: string;
  name?: string;
}): Promise<void> {
  const result = await callJson<{ token: string }>('/auth/password/signup', {
    email: args.email,
    password: args.password,
    name: args.name,
  });
  await bindSession(result.token);
}

async function login(args: { email: string; password: string }): Promise<void> {
  const result = await callJson<{ token: string }>('/auth/password/login', {
    email: args.email,
    password: args.password,
  });
  await bindSession(result.token);
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

function requireConn(): DbConnection {
  if (!conn) throw new Error('chat.disconnected');
  return conn;
}

function clearHeartbeat(): void {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer);
    heartbeatTimer = null;
  }
}

function scheduleHeartbeat(): void {
  clearHeartbeat();
  if (!authUser) return;
  heartbeatTimer = setInterval(() => {
    if (!conn || !authUser) return;
    try {
      void conn.reducers.heartbeat({}).catch(() => undefined);
    } catch {
      // A later heartbeat retries after transient connection failures.
    }
  }, HEARTBEAT_INTERVAL_MS);
}

function scheduleReconnect(): void {
  if (reconnectTimer != null) return;
  const delay =
    RECONNECT_DELAYS_MS[
      Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)
    ];
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    reconnectAttempt++;
    main().catch(err => {
      emitConnectionState('error', normalizeError(err));
      scheduleReconnect();
    });
  }, delay);
}

const STDB_TOKEN_KEY = 'chat:stdb_token';

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

function connect(cfg: ServerConfig): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    const priorToken = loadStdbToken();
    DbConnection.builder()
      .withUri(cfg.spacetimeUri)
      .withDatabaseName(cfg.databaseName)
      .withToken(priorToken)
      .onConnect((connection, _identity, token) => {
        if (token) saveStdbToken(token);
        resolve(connection);
      })
      .onDisconnect((_ctx, err) => {
        conn = null;
        clearHeartbeat();
        emitConnectionState('error', err?.message ?? 'disconnected');
        scheduleReconnect();
      })
      .onConnectError((_ctx, err) => reject(err))
      .build();
  });
}

function subscribeToTables(connection: DbConnection): void {
  connection
    .subscriptionBuilder()
    .onApplied(() => emitPresenceState())
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      tables.myChatUsers,
      tables.myServers,
      tables.serverDirectory,
      tables.myServerMembers,
      tables.myPresenceEntries,
      tables.myRooms,
      tables.myRoomMembers,
      tables.myRoomMessages,
      tables.myRoomMessageReactions,
      tables.myRoomAttachments,
      tables.myMessageThreads,
      tables.myThreadMessages,
      tables.myRoomReadCursors,
      tables.auth.myAuthUser,
      tables.myRateLimitStatus,
    ]);
}

function registerRowCallbacks(connection: DbConnection): void {
  const reRender = () => emitPresenceState();
  const tableAccessors = [
    connection.db.myChatUsers,
    connection.db.myRooms,
    connection.db.myRoomMembers,
    connection.db.myRoomMessages,
    connection.db.myRoomMessageReactions,
    connection.db.myRoomAttachments,
    connection.db.myServerMembers,
    connection.db.serverDirectory,
    connection.db.myMessageThreads,
    connection.db.myThreadMessages,
    connection.db.myRoomReadCursors,
    connection.db.myPresenceEntries,
    connection.db.myRateLimitStatus,
  ];
  for (const t of tableAccessors) {
    t.onInsert(reRender);
    t.onUpdate(reRender);
    t.onDelete(reRender);
  }

  connection.db.myServers.onInsert(reRender);
  connection.db.myServers.onUpdate(reRender);
  connection.db.myServers.onDelete((_ctx: EventContext, row: Server) => {
    if (activeServerId === row.id) {
      activeServerId = null;
      activeRoomId = null;
    }
    emitPresenceState();
  });

  const syncUserFromRow = (row: AuthUserRow) => {
    if (!authUser || row.userId !== authUser.userId) return;
    authUser = {
      userId: row.userId,
      email: row.email,
      emailVerified: row.emailVerified,
      name: row.name ?? undefined,
      image: row.image ?? undefined,
    };
    emitAuthState();
  };
  connection.db['auth.my_auth_user'].onInsert(
    (_ctx: EventContext, row: AuthUserRow) => syncUserFromRow(row)
  );
  connection.db['auth.my_auth_user'].onUpdate(
    (_ctx: EventContext, _old: AuthUserRow, neu: AuthUserRow) =>
      syncUserFromRow(neu)
  );
  connection.db['auth.my_auth_user'].onDelete(
    (_ctx: EventContext, row: AuthUserRow) => {
      if (!authUser || row.userId !== authUser.userId) return;
      authUser = null;
      sessionExpiresAt = undefined;
      emitAuthState();
      emitPresenceState();
    }
  );
}

async function bindSession(
  sessionToken: string,
  refreshedUser?: AuthUser,
  exp?: number
): Promise<void> {
  const c = requireConn();
  await c.reducers['auth.linkConnection']({ sessionToken });
  if (!refreshedUser) {
    const meRes = await callJson<AuthMeResponse>('/auth/me');
    refreshedUser = meRes.user;
    exp = meRes.sessionExpiresAt;
  }
  authUser = refreshedUser;
  sessionExpiresAt = exp;
  await c.reducers.heartbeat({});
  emitAuthState();
  emitPresenceState();
  scheduleHeartbeat();
}

async function restoreSession(): Promise<boolean> {
  try {
    const refreshed = await callJson<AuthRefreshResponse>(
      '/auth/session/refresh',
      {}
    );
    await bindSession(
      refreshed.token,
      refreshed.user,
      refreshed.sessionExpiresAt
    );
    return true;
  } catch {
    authUser = null;
    sessionExpiresAt = undefined;
    emitAuthState();
    clearHeartbeat();
    return false;
  }
}

function installApi(): void {
  window.chat = {
    setDisplayName: (displayName: string) => {
      return requireConn().reducers.setDisplayName({ displayName });
    },
    setStatus: status => {
      // UI passes lowercase strings; map to ChatUserStatus enum tags.
      const tag = (status.charAt(0).toUpperCase() + status.slice(1)) as
        | 'Online'
        | 'Away'
        | 'Dnd'
        | 'Invisible';
      return requireConn().reducers.setStatus({ status: { tag } });
    },
    createServer: (name: string) => {
      return requireConn().reducers.createServer({ name });
    },
    renameServer: (serverId: bigint, name: string) => {
      return requireConn().reducers.renameServer({ serverId, name });
    },
    deleteServer: (serverId: bigint) => {
      return requireConn().reducers.deleteServer({ serverId });
    },
    joinServer: (serverId: bigint) => {
      return requireConn().reducers.joinServer({ serverId });
    },
    leaveServer: (serverId: bigint) => {
      return requireConn().reducers.leaveServer({ serverId });
    },
    setActiveServer: (serverId: bigint | null) => {
      activeServerId = serverId;
      activeRoomId = null;
      emitPresenceState();
    },
    createRoom: (
      serverId: bigint,
      name: string,
      isPrivate: boolean,
      category?: string
    ) => {
      return requireConn().reducers.createRoom({
        serverId,
        name,
        isPrivate,
        category,
      });
    },
    joinRoom: (roomId: bigint) => {
      return requireConn().reducers.joinRoom({ roomId });
    },
    addRoomMember: (roomId: bigint, userId: string) => {
      return requireConn().reducers.addRoomMember({ roomId, userId });
    },
    leaveRoom: (roomId: bigint) => {
      return requireConn().reducers.leaveRoom({ roomId });
    },
    sendMessage: (
      roomId: bigint,
      content: string,
      replyToMessageId?: bigint,
      atts?: AttachmentInput[]
    ) => {
      return requireConn().reducers.sendMessage({
        roomId,
        content,
        replyToMessageId,
        attachments: atts ?? [],
      });
    },
    sendThreadMessage: (rootMessageId: bigint, content: string) => {
      return requireConn().reducers.sendThreadMessage({
        rootMessageId,
        content,
      });
    },
    getAttachmentFile: async (fileId: bigint) => {
      return await requireConn().procedures.getAttachmentFile({ fileId });
    },
    editMessage: (messageId: bigint, content: string) => {
      return requireConn().reducers.editMessage({ messageId, content });
    },
    deleteMessage: (messageId: bigint) => {
      return requireConn().reducers.deleteMessage({ messageId });
    },
    editThreadMessage: (threadMessageId: bigint, content: string) => {
      return requireConn().reducers.editThreadMessage({
        threadMessageId,
        content,
      });
    },
    deleteThreadMessage: (threadMessageId: bigint) => {
      return requireConn().reducers.deleteThreadMessage({ threadMessageId });
    },
    renameRoom: (roomId: bigint, name: string) => {
      return requireConn().reducers.renameRoom({ roomId, name });
    },
    setRoomCategory: (roomId: bigint, category?: string) => {
      return requireConn().reducers.setRoomCategory({ roomId, category });
    },
    setRoomPrivacy: (roomId: bigint, isPrivate: boolean) => {
      return requireConn().reducers.setRoomPrivacy({ roomId, isPrivate });
    },
    deleteRoom: (roomId: bigint) => {
      return requireConn().reducers.deleteRoom({ roomId });
    },
    startTyping: (roomId: bigint) => {
      return requireConn().reducers.startTyping({ roomId });
    },
    stopTyping: (roomId: bigint) => {
      return requireConn().reducers.stopTyping({ roomId });
    },
    markRoomRead: (roomId: bigint) => {
      return requireConn().reducers.markRoomRead({ roomId });
    },
    toggleReaction: (messageId: bigint, emoji: string) => {
      return requireConn().reducers.toggleReaction({ messageId, emoji });
    },
    pinMessage: (messageId: bigint) => {
      return requireConn().reducers.pinMessage({ messageId });
    },
    unpinMessage: (messageId: bigint) => {
      return requireConn().reducers.unpinMessage({ messageId });
    },
    searchMessages: async (roomId: bigint, query: string) => {
      return await requireConn().procedures.searchMessages({ roomId, query });
    },
    setActiveRoom: (roomId: bigint | null) => {
      activeRoomId = roomId;
      emitPresenceState();
    },
    heartbeat: () => {
      return requireConn().reducers.heartbeat({});
    },
    signup,
    login,
    logout: async () => {
      try {
        await conn?.reducers['auth.unlinkConnection']({});
      } catch {
        /* The reload below drops this connection either way. */
      }
      await callJson('/auth/logout', {});
      // Reconnect with a new SpacetimeDB identity so the next sign-in on this
      // browser starts from a fresh connection.
      try {
        localStorage.removeItem(STDB_TOKEN_KEY);
      } catch {
        /* Storage can be unavailable. */
      }
      window.location.reload();
    },
    oauthStart,
    forgotPassword,
    resetPassword,
    requestEmailVerify: async () => {
      await callJson('/auth/email/verify-request', {});
    },
    setProfile: args => {
      return requireConn().reducers['auth.updateProfile']({
        name: args.name,
        image: args.image,
      });
    },
  };
}

const authResult = authUrlState(window.location);
const authPanelRoot = document.getElementById('auth-panel');
if (!authPanelRoot) throw new Error('missing_auth_panel');
const authPanel = mountAuthPanel(authPanelRoot, {
  productName: 'Chat',
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
  emitConnectionState('connecting');
  if (!config) config = await loadServerConfig();

  const connection = await connect(config);
  conn = connection;
  reconnectAttempt = 0;
  emitConnectionState('connected');
  registerRowCallbacks(connection);
  subscribeToTables(connection);
  installApi();
  await restoreSession();
  window.dispatchEvent(new CustomEvent('chat:ready'));
}

main().catch(err => {
  emitConnectionState('error', normalizeError(err));
  scheduleReconnect();
});
