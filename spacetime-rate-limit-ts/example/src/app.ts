import {
  DbConnection,
  tables,
  type ErrorContext,
  type EventContext,
} from './module_bindings/app/index.ts';

type TimestampLike = { microsSinceUnixEpoch: bigint };

type ReactorState = {
  singleton: boolean;
  energy: bigint;
  reactorLevel: number;
  upgradeCount: number;
  powerUpgradeCount: number;
  coolingUpgradeCount: number;
  capacityUpgradeCount: number;
  chargeUpgradeCount: number;
  bayUpgradeCount: number;
  combo: number;
  bestCombo: number;
  heat: number;
  heatCapacity: number;
  coolingPerSecond: number;
  tapHeatGain: number;
  overheated: boolean;
  updatedAt: TimestampLike;
};

type ReactorEvent = {
  id: bigint;
  identity: unknown;
  actorName: string;
  actorColor: string;
  kind: string;
  scope: string;
  message: string;
  allowed: boolean;
  energyDelta: bigint;
  retryAfterSeconds: number;
  createdAt: TimestampLike;
};

type ReactorPlayer = {
  identity: unknown;
  displayName: string;
  color: string;
  contributedEnergy: bigint;
  taps: number;
  surges: number;
  coolantUses: number;
  upgradesBought: number;
  joinedAt: TimestampLike;
  updatedAt: TimestampLike;
};

type ReactorLimitStatus = {
  scope: string;
  label: string;
  limit: number;
  windowSeconds: number;
  used: number;
  remaining: number;
  resetAt?: TimestampLike;
};

type ReactorShopItem = {
  slot: number;
  id: string;
  name: string;
  description: string;
  effect: string;
  cost: bigint;
  available: boolean;
};

type RateLimitDemoConfig = {
  singleton: boolean;
  retainEvents: number;
  eventPruneBatch: number;
  updatedAt: TimestampLike;
};

type ReactorActionResult = {
  allowed: boolean;
  action: string;
  message: string;
  energy: bigint;
  energyDelta: bigint;
  retryAfterSeconds: number;
  resetAt: TimestampLike;
};

type ReactorActions = {
  start: () => Promise<ReactorActionResult>;
  tap: () => Promise<ReactorActionResult>;
  overcharge: () => Promise<ReactorActionResult>;
  buyUpgrade: (upgradeId: string) => Promise<ReactorActionResult>;
  repair: () => Promise<ReactorActionResult>;
  setPlayerColor: (color: string) => Promise<void>;
  runSweep: (maxRows?: number) => Promise<number>;
  resetDemo: () => Promise<void>;
  updateConfig: (args: {
    sweepBatch?: number;
    retainEvents?: number;
    eventPruneBatch?: number;
  }) => Promise<void>;
};

declare global {
  interface Window {
    reactor?: ReactorActions;
  }
}

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

type TableEvents<T> = {
  iter(): Iterable<T>;
  onInsert(cb: (ctx: EventContext, row: T) => void): void;
  onUpdate(cb: (ctx: EventContext, old: T, row: T) => void): void;
  onDelete(cb: (ctx: EventContext, row: T) => void): void;
};

type SingletonTable<T> = {
  singleton: { find(key: boolean): T | null | undefined };
  onInsert(cb: (ctx: EventContext, row: T) => void): void;
  onUpdate(cb: (ctx: EventContext, old: T, row: T) => void): void;
  onDelete?(cb: (ctx: EventContext, row: T) => void): void;
};

type NamespacedDb = DbConnection['db'] & {
  reactorState: TableEvents<ReactorState>;
  reactorEvents: TableEvents<ReactorEvent>;
  reactorLimitStatus: TableEvents<ReactorLimitStatus>;
  reactorPlayers: TableEvents<ReactorPlayer>;
  reactorShop: TableEvents<ReactorShopItem>;
  rateLimitDemoConfig: SingletonTable<RateLimitDemoConfig>;
};

let currentConn: DbConnection | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectAttempt = 0;
let serverConfig: ServerConfig | null = null;
let currentIdentityHex: string | null = null;

const RECONNECT_DELAYS_MS = [1000, 2000, 5000, 10000, 15000];
const CONNECT_TIMEOUT_MS = 8000;
const TOKEN_STORAGE_PREFIX = 'reactor-clicker.stdb-token';

function emitConnectionState(
  state: 'connecting' | 'connected' | 'error',
  detail?: string
): void {
  window.dispatchEvent(
    new CustomEvent('reactor:connState', { detail: { state, detail } })
  );
}

function emitReactorState(): void {
  if (!currentConn) {
    window.dispatchEvent(
      new CustomEvent('reactor:data', {
        detail: {
          state: null,
          events: [],
          players: [],
          statuses: [],
          shop: [],
          demoConfig: null,
          currentIdentityHex: null,
        },
      })
    );
    return;
  }

  const db = currentConn.db as NamespacedDb;
  window.dispatchEvent(
    new CustomEvent('reactor:data', {
      detail: {
        state: [...db.reactorState.iter()][0] ?? null,
        events: [...db.reactorEvents.iter()].sort((a, b) =>
          a.id < b.id ? 1 : a.id > b.id ? -1 : 0
        ),
        players: [...db.reactorPlayers.iter()].sort((a, b) => {
          if (a.contributedEnergy === b.contributedEnergy)
            return a.displayName.localeCompare(b.displayName);
          return a.contributedEnergy < b.contributedEnergy ? 1 : -1;
        }),
        statuses: [...db.reactorLimitStatus.iter()].sort((a, b) =>
          a.label.localeCompare(b.label)
        ),
        shop: [...db.reactorShop.iter()].sort((a, b) => a.slot - b.slot),
        demoConfig: db.rateLimitDemoConfig.singleton.find(true) ?? null,
        currentIdentityHex,
      },
    })
  );
}

function emitReactorEvent(row: ReactorEvent): void {
  window.dispatchEvent(
    new CustomEvent('reactor:eventInserted', { detail: { event: row } })
  );
}

async function loadServerConfig(): Promise<ServerConfig> {
  const r = await fetch('/api/config');
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  return (await r.json()) as ServerConfig;
}

function isStoredTokenAuthError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return /unauthorized|verify token|websocket-token/i.test(message);
}

function connectOnce(
  cfg: ServerConfig,
  tokenKey: string,
  token?: string
): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(new Error('stdb.connect_timeout'));
    }, CONNECT_TIMEOUT_MS);
    const settle = (fn: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      fn();
    };

    const builder = DbConnection.builder()
      .withUri(cfg.spacetimeUri)
      .withDatabaseName(cfg.databaseName)
      .onConnect((conn, identity, nextToken) =>
        settle(() => {
          currentIdentityHex =
            typeof (identity as { toHexString?: () => string }).toHexString ===
            'function'
              ? (identity as { toHexString: () => string }).toHexString()
              : String(identity);
          window.localStorage.setItem(tokenKey, nextToken);
          resolve(conn);
        })
      )
      .onDisconnect((_ctx, err) => {
        currentConn = null;
        window.reactor = undefined;
        emitConnectionState('error', err?.message ?? 'disconnected');
        scheduleReconnect();
      })
      .onConnectError((_ctx, err) => settle(() => reject(err)));
    if (token) builder.withToken(token);
    builder.build();
  });
}

async function connect(cfg: ServerConfig): Promise<DbConnection> {
  const tokenKey = `${TOKEN_STORAGE_PREFIX}.${cfg.spacetimeUri}.${cfg.databaseName}`;
  const token = window.localStorage.getItem(tokenKey) ?? undefined;
  try {
    return await connectOnce(cfg, tokenKey, token);
  } catch (err) {
    if (token && isStoredTokenAuthError(err)) {
      window.localStorage.removeItem(tokenKey);
      return connectOnce(cfg, tokenKey);
    }
    throw err;
  }
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
      emitConnectionState(
        'error',
        err instanceof Error ? err.message : String(err)
      );
      scheduleReconnect();
    });
  }, delay);
}

type TableSyncState = {
  subscriptionApplied: boolean;
  seenEventIds: Set<string>;
};

function subscribeToTables(
  connection: DbConnection,
  state: TableSyncState
): void {
  const db = connection.db as NamespacedDb;
  connection
    .subscriptionBuilder()
    .onApplied(() => {
      for (const row of db.reactorEvents.iter()) {
        state.seenEventIds.add(row.id.toString());
      }
      state.subscriptionApplied = true;
      emitReactorState();
    })
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      tables.reactorState,
      tables.reactorEvents,
      tables.reactorLimitStatus,
      tables.reactorPlayers,
      tables.reactorShop,
      tables.rateLimitDemoConfig,
    ]);
}

function registerRowCallbacks(
  connection: DbConnection,
  state: TableSyncState
): void {
  const db = connection.db as NamespacedDb;
  db.reactorState.onInsert(() => emitReactorState());
  db.reactorState.onUpdate(() => emitReactorState());
  db.reactorState.onDelete(() => emitReactorState());
  db.reactorEvents.onInsert((_ctx, row) => {
    const id = row.id.toString();
    const isNewLiveEvent =
      state.subscriptionApplied && !state.seenEventIds.has(id);
    state.seenEventIds.add(id);
    if (isNewLiveEvent) emitReactorEvent(row);
    emitReactorState();
  });
  db.reactorEvents.onUpdate(() => emitReactorState());
  db.reactorEvents.onDelete(() => emitReactorState());
  db.reactorLimitStatus.onInsert(() => emitReactorState());
  db.reactorLimitStatus.onUpdate(() => emitReactorState());
  db.reactorLimitStatus.onDelete(() => emitReactorState());
  db.reactorPlayers.onInsert(() => emitReactorState());
  db.reactorPlayers.onUpdate(() => emitReactorState());
  db.reactorPlayers.onDelete(() => emitReactorState());
  db.reactorShop.onInsert(() => emitReactorState());
  db.reactorShop.onUpdate(() => emitReactorState());
  db.reactorShop.onDelete(() => emitReactorState());
  db.rateLimitDemoConfig.onInsert(() => emitReactorState());
  db.rateLimitDemoConfig.onUpdate(() => emitReactorState());
  db.rateLimitDemoConfig.onDelete?.(() => emitReactorState());
}

function requireConn(): DbConnection {
  if (!currentConn) throw new Error('stdb.disconnected');
  return currentConn;
}

function installReactorActions(): ReactorActions {
  const actions: ReactorActions = {
    start: async () => requireConn().procedures.startReactor({}),
    tap: async () => requireConn().procedures.tapReactor({}),
    overcharge: async () => requireConn().procedures.overcharge({}),
    buyUpgrade: async (upgradeId: string) =>
      requireConn().procedures.buyUpgrade({ upgradeId }),
    repair: async () => requireConn().procedures.repairReactor({}),
    setPlayerColor: async (color: string) => {
      requireConn().reducers.setPlayerColor({ color });
    },
    runSweep: async (maxRows?: number) =>
      requireConn().procedures.runSweep({ maxRows }),
    resetDemo: async () => {
      requireConn().reducers.resetDemo({});
    },
    updateConfig: async args => {
      requireConn().reducers.updateConfig({
        sweepBatch: args.sweepBatch,
        retainEvents: args.retainEvents,
        eventPruneBatch: args.eventPruneBatch,
      });
    },
  };
  window.reactor = actions;
  return actions;
}

async function main(): Promise<void> {
  window.reactor = undefined;
  emitConnectionState('connecting');
  if (!serverConfig) {
    serverConfig = await loadServerConfig();
  }
  try {
    const conn = await connect(serverConfig);
    currentConn = conn;
    reconnectAttempt = 0;
    const tableSyncState: TableSyncState = {
      subscriptionApplied: false,
      seenEventIds: new Set(),
    };
    registerRowCallbacks(conn, tableSyncState);
    subscribeToTables(conn, tableSyncState);
    const reactor = installReactorActions();
    window.dispatchEvent(new CustomEvent('reactor:ready'));
    emitConnectionState('connected');
    reactor.start().catch((err: unknown) => {
      emitConnectionState(
        'error',
        err instanceof Error ? err.message : String(err)
      );
    });
  } catch (err) {
    currentConn = null;
    window.reactor = undefined;
    throw err;
  }
}

main().catch(err => {
  console.error('reactor connection failed', err);
  emitConnectionState(
    'error',
    err instanceof Error ? err.message : String(err)
  );
  scheduleReconnect();
});
