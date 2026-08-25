import {
  DbConnection,
  type ErrorContext,
  type EventContext,
} from './codegen/app/index.ts';

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
  stdbUri: string;
  appDatabase: string;
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

function broadcastConn(
  state: 'connecting' | 'connected' | 'error',
  detail?: string
): void {
  window.dispatchEvent(
    new CustomEvent('reactor:connState', { detail: { state, detail } })
  );
}

function broadcastState(): void {
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

function broadcastEventInsert(row: ReactorEvent): void {
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

function openConnection(
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
      .withUri(cfg.stdbUri)
      .withDatabaseName(cfg.appDatabase)
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
        broadcastConn('error', err?.message ?? 'disconnected');
        scheduleReconnect();
      })
      .onConnectError((_ctx, err) => settle(() => reject(err)));
    if (token) builder.withToken(token);
    builder.build();
  });
}

async function connect(cfg: ServerConfig): Promise<DbConnection> {
  const tokenKey = `${TOKEN_STORAGE_PREFIX}.${cfg.stdbUri}.${cfg.appDatabase}`;
  const token = window.localStorage.getItem(tokenKey) ?? undefined;
  try {
    return await openConnection(cfg, tokenKey, token);
  } catch (err) {
    if (token && isStoredTokenAuthError(err)) {
      window.localStorage.removeItem(tokenKey);
      return openConnection(cfg, tokenKey);
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
    run().catch(err => {
      broadcastConn('error', err instanceof Error ? err.message : String(err));
      scheduleReconnect();
    });
  }, delay);
}

function wireDataHandlers(conn: DbConnection): void {
  const db = conn.db as NamespacedDb;
  let subscriptionsApplied = false;
  const seenEventIds = new Set<string>();

  conn
    .subscriptionBuilder()
    .onApplied(() => {
      for (const row of db.reactorEvents.iter()) {
        seenEventIds.add(row.id.toString());
      }
      subscriptionsApplied = true;
      broadcastState();
    })
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      'SELECT * FROM reactor_state',
      'SELECT * FROM reactor_events',
      'SELECT * FROM reactor_limit_status',
      'SELECT * FROM reactor_players',
      'SELECT * FROM reactor_shop',
      'SELECT * FROM rate_limit_demo_config',
    ]);

  db.reactorState.onInsert(() => broadcastState());
  db.reactorState.onUpdate(() => broadcastState());
  db.reactorState.onDelete(() => broadcastState());
  db.reactorEvents.onInsert((_ctx, row) => {
    const id = row.id.toString();
    const isNewLiveEvent = subscriptionsApplied && !seenEventIds.has(id);
    seenEventIds.add(id);
    if (isNewLiveEvent) broadcastEventInsert(row);
    broadcastState();
  });
  db.reactorEvents.onUpdate(() => broadcastState());
  db.reactorEvents.onDelete(() => broadcastState());
  db.reactorLimitStatus.onInsert(() => broadcastState());
  db.reactorLimitStatus.onUpdate(() => broadcastState());
  db.reactorLimitStatus.onDelete(() => broadcastState());
  db.reactorPlayers.onInsert(() => broadcastState());
  db.reactorPlayers.onUpdate(() => broadcastState());
  db.reactorPlayers.onDelete(() => broadcastState());
  db.reactorShop.onInsert(() => broadcastState());
  db.reactorShop.onUpdate(() => broadcastState());
  db.reactorShop.onDelete(() => broadcastState());
  db.rateLimitDemoConfig.onInsert(() => broadcastState());
  db.rateLimitDemoConfig.onUpdate(() => broadcastState());
  db.rateLimitDemoConfig.onDelete?.(() => broadcastState());
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

async function run(): Promise<void> {
  window.reactor = undefined;
  broadcastConn('connecting');
  if (!serverConfig) {
    serverConfig = await loadServerConfig();
  }
  try {
    const conn = await connect(serverConfig);
    currentConn = conn;
    reconnectAttempt = 0;
    wireDataHandlers(conn);
    const reactor = installReactorActions();
    window.dispatchEvent(new CustomEvent('reactor:ready'));
    broadcastConn('connected');
    reactor.start().catch((err: unknown) => {
      broadcastConn('error', err instanceof Error ? err.message : String(err));
    });
  } catch (err) {
    currentConn = null;
    window.reactor = undefined;
    throw err;
  }
}

run().catch(err => {
  console.error('reactor connection failed', err);
  broadcastConn('error', err instanceof Error ? err.message : String(err));
  scheduleReconnect();
});
