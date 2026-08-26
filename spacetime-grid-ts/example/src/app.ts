import {
  authUrlState,
  clearAuthResultParams,
  mountAuthPanel,
} from '@spacetimedb/example-ui';
import '@spacetimedb/example-ui/styles.css';
import {
  DbConnection,
  tables,
  type ErrorContext,
  type SubscriptionHandle,
} from './module_bindings/app';
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

interface ReachableCell {
  x: number;
  y: number;
  cost: number;
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
      requestEmailVerify: () => Promise<void>;
    };
    grid?: {
      createMatch: (
        vsAi: boolean
      ) => Promise<{ matchId: bigint; gridId: bigint }>;
      joinMatch: (matchId: bigint) => Promise<void>;
      setActiveMatch: (matchId: bigint | null) => void;
      moveUnit: (
        entityId: bigint,
        toX: number,
        toY: number
      ) => Promise<{ path: Array<{ x: number; y: number }> }>;
      attackUnit: (attackerId: bigint, targetId: bigint) => Promise<void>;
      endTurn: (matchId: bigint) => Promise<void>;
      getCellsInRange: (
        gridId: bigint,
        originX: number,
        originY: number,
        maxCost: number
      ) => Promise<ReachableCell[]>;
      AI_BOT_USER_ID: string;
    };
  }
}

type ConnState = 'idle' | 'connecting' | 'connected' | 'error';

let currentConn: DbConnection | null = null;
let globalSub: SubscriptionHandle | null = null;
let matchSub: SubscriptionHandle | null = null;
let activeMatchId: bigint | null = null;
type ServerConfig = {
  spacetimeUri: string;
  databaseName: string;
  oauth?: {
    google?: boolean;
    github?: boolean;
  };
};

let serverCfg: ServerConfig | null = null;

let currentUser: AuthUser | null = null;
let currentExp: number | undefined;

let reconnectAttempt = 0;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000, 15000];

function emitAppEvent(name: string, detail: unknown): void {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}
function emitConnectionState(state: ConnState, detail?: string): void {
  emitAppEvent('grid:conn', { state, detail });
}
function emitAuthState(): void {
  emitAppEvent('grid:auth', {
    user: currentUser,
    sessionExpiresAt: currentExp,
  });
}
function emitGridState(): void {
  const myUserId = currentUser?.userId;
  if (!currentConn) {
    emitAppEvent('grid:state', {
      myUserId,
      matches: [],
      activeMatchId,
      activeMatch: null,
      activeGrid: null,
      units: [],
      entities: [],
      cells: [],
      unitTypes: [],
      actors: [],
      openMatches: [],
    });
    return;
  }
  const c = currentConn;
  const matchList = [...c.db.myMatches.iter()].sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch as bigint;
    const bv = b.createdAt.microsSinceUnixEpoch as bigint;
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
  const activeMatch =
    activeMatchId !== null
      ? (matchList.find(m => m.matchId === activeMatchId) ?? null)
      : null;
  const activeGrid = activeMatch
    ? ([...c.db.myGrids.iter()].find(g => g.id === activeMatch.gridId) ?? null)
    : null;
  const activeUnits =
    activeMatchId !== null
      ? [...c.db.myPlayerUnits.iter()].filter(u => u.matchId === activeMatchId)
      : [];
  const activeEntities = activeGrid
    ? [...c.db.myGridEntities.iter()].filter(e => e.gridId === activeGrid.id)
    : [];
  const activeCells = activeGrid
    ? [...c.db.myCellStates.iter()].filter(c2 => c2.gridId === activeGrid.id)
    : [];
  emitAppEvent('grid:state', {
    myUserId,
    matches: matchList,
    participants: [...c.db.myMatchParticipants.iter()],
    activeMatchId,
    activeMatch,
    activeGrid,
    units: activeUnits,
    entities: activeEntities,
    cells: activeCells,
    unitTypes: [...c.db.unitType.iter()],
    actors: [...c.db.actorDirectory.iter()],
    openMatches: [...c.db.lobbyOpenMatches.iter()],
  });
}

function requireConn(): DbConnection {
  if (!currentConn) throw new Error('STDB not connected');
  return currentConn;
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
    /* empty */
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
  const cfg = (await res.json()) as ServerConfig;
  authPanel.setProviders({
    google: Boolean(cfg.oauth?.google),
    github: Boolean(cfg.oauth?.github),
  });
  return cfg;
}

function connect(uri: string, databaseName: string): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(uri)
      .withDatabaseName(databaseName)
      .onConnect(c => resolve(c))
      .onDisconnect((_ctx, err) => {
        emitConnectionState('error', err?.message ?? 'disconnected');
        currentConn = null;
        globalSub = null;
        matchSub = null;
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

function setActiveMatch(matchId: bigint | null): void {
  if (activeMatchId === matchId) return;
  activeMatchId = matchId;

  if (matchSub) {
    matchSub.unsubscribe();
    matchSub = null;
  }

  if (matchId === null || !currentConn) {
    emitGridState();
    return;
  }

  const m = currentConn
    ? [...currentConn.db.myMatches.iter()].find(x => x.matchId === matchId)
    : undefined;
  if (!m) {
    emitGridState();
    return;
  }

  // Per-active-match subscription for the grid entities and cell states in
  // this match's grid, plus the player_unit rows for this match. The match
  // row + unit_type rows + auth_user rows are already in the global sub.
  matchSub = currentConn
    .subscriptionBuilder()
    .onApplied(() => emitGridState())
    .onError((ctx: ErrorContext) => console.error('match sub error', ctx.event))
    .subscribe([
      tables.myPlayerUnits.where(row => row.matchId.eq(matchId)),
      tables.myGridEntities.where(row => row.gridId.eq(m.gridId)),
      tables.myCellStates.where(row => row.gridId.eq(m.gridId)),
      tables.myGrids.where(row => row.id.eq(m.gridId)),
    ]);
}

function registerRowCallbacks(connection: DbConnection): void {
  const tableAccessors = [
    connection.db.myMatches,
    connection.db.myMatchParticipants,
    connection.db.myPlayerUnits,
    connection.db.unitType,
    connection.db.myGrids,
    connection.db.myGridEntities,
    connection.db.myCellStates,
    connection.db.actorDirectory,
    connection.db.lobbyOpenMatches,
  ];
  for (const t of tableAccessors) {
    t.onInsert(() => emitGridState());
    t.onUpdate(() => emitGridState());
    t.onDelete(() => emitGridState());
  }
}

function subscribeToTables(connection: DbConnection): SubscriptionHandle {
  return connection
    .subscriptionBuilder()
    .onApplied(() => emitGridState())
    .onError((ctx: ErrorContext) =>
      console.error('global sub error', ctx.event)
    )
    .subscribe([
      tables.myMatches,
      tables.myMatchParticipants,
      tables.unitType,
      tables.actorDirectory,
      tables.lobbyOpenMatches,
    ]);
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

      emitGridState();

      registerRowCallbacks(conn);

      globalSub = subscribeToTables(conn);

      // Re-open per-match subscription if a match was active before reconnect.
      const previousActive = activeMatchId;
      activeMatchId = null;
      matchSub = null;
      if (previousActive !== null) setActiveMatch(previousActive);
    } catch (err) {
      emitConnectionState(
        'error',
        err instanceof Error ? err.message : String(err)
      );
      return;
    }
  }

  try {
    await currentConn.reducers.linkConnection({ sessionToken: token });
  } catch (err) {
    console.warn('link_connection failed', err);
  }

  emitAuthState();
  emitGridState();
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
  globalSub?.unsubscribe();
  globalSub = null;
  matchSub?.unsubscribe();
  matchSub = null;
  if (currentConn) {
    try {
      await currentConn.reducers.unlinkConnection({});
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
  activeMatchId = null;
  emitAuthState();
  emitGridState();
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

const authResult = authUrlState(window.location);
const authPanelRoot = document.getElementById('auth-panel');
if (!authPanelRoot) throw new Error('missing_auth_panel');
const authPanel = mountAuthPanel(authPanelRoot, {
  productName: 'Grid',
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
    requestEmailVerify,
  };

  window.grid = {
    AI_BOT_USER_ID: 'ai-bot-001',
    createMatch: async vsAi => requireConn().procedures.createMatch({ vsAi }),
    joinMatch: async matchId => {
      await requireConn().procedures.joinMatch({ matchId });
    },
    setActiveMatch,
    moveUnit: async (entityId, toX, toY) => {
      const r = await requireConn().procedures.moveUnit({ entityId, toX, toY });
      return { path: r.path };
    },
    attackUnit: async (attackerId, targetId) => {
      await requireConn().procedures.attackUnit({ attackerId, targetId });
    },
    endTurn: async matchId => {
      await requireConn().procedures.endTurn({ matchId });
      // If the new active seat belongs to the built-in AI, prod it to play.
      // Pause briefly so the player can see the turn flip in the UI.
      const conn = currentConn;
      const m = conn
        ? [...conn.db.myMatches.iter()].find(x => x.matchId === matchId)
        : undefined;
      const seatUserId =
        conn && m
          ? [...conn.db.myMatchParticipants.iter()].find(
              p => p.matchId === matchId && p.seatIdx === m.currentSeatIdx
            )?.userId
          : undefined;
      if (m && m.status.tag === 'Active' && seatUserId === 'ai-bot-001') {
        await new Promise(r => setTimeout(r, 400));
        try {
          const result = await requireConn().procedures.aiTakeTurn({ matchId });
          // Hand the events to the renderer so it can sequence:
          // move animation → pause → attack flash → target HP drop / death.
          emitAppEvent('grid:ai-events', { events: result.events });
        } catch (err) {
          console.error('ai_take_turn failed:', err);
        }
      }
    },
    getCellsInRange: async (gridId, originX, originY, maxCost) => {
      const r = await requireConn().procedures.getCellsInRange({
        gridId,
        originX,
        originY,
        maxCost,
      });
      return r.cells;
    },
  };

  emitConnectionState('idle');
  serverCfg = await loadServerConfig();
  await restoreSession();
  emitAppEvent('grid:ready', {});
}

main().catch(err => {
  console.error(err);
  emitConnectionState(
    'error',
    err instanceof Error ? err.message : String(err)
  );
});
