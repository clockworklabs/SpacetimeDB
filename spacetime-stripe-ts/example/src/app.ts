import {
  DbConnection,
  tables,
  type EventContext,
  type ErrorContext,
} from './module_bindings/app';

declare global {
  interface Window {
    stdb?: {
      getOrCreateCustomer: (args: {
        userId: string;
        email?: string;
        name?: string;
      }) => Promise<{ customerId: string; isNew: boolean }>;
      createCheckoutSession: (args: {
        items: Array<{ priceId: string; quantity: number }>;
        customerId?: string;
        mode: 'payment' | 'subscription' | string;
        successUrl: string;
        cancelUrl: string;
        metadataJson?: string;
        subscriptionMetadataJson?: string;
        paymentIntentMetadataJson?: string;
      }) => Promise<{ sessionId: string; url?: string }>;
      validatePrice: (priceId: string) => Promise<{
        valid: boolean;
        active?: boolean;
        message?: string;
        code?: string;
        errorType?: string;
      }>;
      getWebhookEventCount: () => Promise<number>;
    };
  }
}

type StoreProductRow = {
  productId: string;
  name: string;
  description: string;
  mode: string;
  priceLabel: string;
  stripePriceId: string | undefined;
  perksJson: string | undefined;
  active: boolean;
  sortOrder: bigint;
  createdAt: { microsSinceUnixEpoch: bigint };
  updatedAt: { microsSinceUnixEpoch: bigint };
};

interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

const products = new Map<string, StoreProductRow>();

function parsePerks(json: string | undefined): string[] {
  if (!json) return [];
  try {
    const parsed = JSON.parse(json);
    return Array.isArray(parsed)
      ? parsed.filter(x => typeof x === 'string')
      : [];
  } catch {
    return [];
  }
}

function broadcastCatalog() {
  const sorted = [...products.values()]
    .filter(p => p.active)
    .sort((a, b) => {
      const so = Number(a.sortOrder - b.sortOrder);
      return so !== 0 ? so : a.productId.localeCompare(b.productId);
    });
  window.dispatchEvent(
    new CustomEvent('stdb:catalog', {
      detail: {
        products: sorted.map(p => ({
          id: p.productId,
          name: p.name,
          description: p.description,
          mode: p.mode,
          priceLabel: p.priceLabel,
          priceId: p.stripePriceId ?? '',
          perks: parsePerks(p.perksJson),
          sortOrder: Number(p.sortOrder),
          active: p.active,
        })),
      },
    })
  );
}

function updateConnState(
  state: 'connecting' | 'connected' | 'error',
  detail?: string
) {
  window.dispatchEvent(
    new CustomEvent('stdb:connState', { detail: { state, detail } })
  );
}

async function loadServerConfig(): Promise<ServerConfig> {
  const r = await fetch('/api/config');
  if (!r.ok) throw new Error(`/api/config returned ${r.status}`);
  return (await r.json()) as ServerConfig;
}

function connect(config: ServerConfig): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(
      () => reject(new Error(`Timed out connecting to ${config.spacetimeUri}`)),
      10000
    );
    DbConnection.builder()
      .withUri(config.spacetimeUri)
      .withDatabaseName(config.databaseName)
      .withCompression('none')
      .onConnect(c => {
        window.clearTimeout(timeout);
        resolve(c);
      })
      .onDisconnect((_ctx, err) => {
        window.clearTimeout(timeout);
        updateConnState('error', err?.message ?? 'disconnected');
      })
      .onConnectError((_ctx, err) => {
        window.clearTimeout(timeout);
        updateConnState('error', 'connect failed (app)');
        reject(err);
      })
      .build();
  });
}

async function api<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    headers:
      body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const payload: unknown = await response.json().catch(() => ({}));
  if (!response.ok) {
    const message =
      isRecord(payload) && typeof payload.error === 'string'
        ? payload.error
        : `${path} returned ${response.status}`;
    throw new Error(message);
  }
  return payload as T;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function registerRowCallbacks(connection: DbConnection): void {
  connection.db.storeProduct.onInsert(
    (_ctx: EventContext, row: StoreProductRow) => {
      products.set(row.productId, row);
      broadcastCatalog();
    }
  );
  connection.db.storeProduct.onUpdate(
    (_ctx: EventContext, _oldRow: StoreProductRow, row: StoreProductRow) => {
      products.set(row.productId, row);
      broadcastCatalog();
    }
  );
  connection.db.storeProduct.onDelete(
    (_ctx: EventContext, row: StoreProductRow) => {
      products.delete(row.productId);
      broadcastCatalog();
    }
  );
}

function subscribeToTables(connection: DbConnection): void {
  connection
    .subscriptionBuilder()
    .onApplied(() => {
      products.clear();
      for (const row of connection.db.storeProduct.iter() as Iterable<StoreProductRow>) {
        products.set(row.productId, row);
      }
      broadcastCatalog();
      updateConnState('connected');
      window.dispatchEvent(new CustomEvent('stdb:ready'));
    })
    .onError((ctx: ErrorContext) => {
      console.error('catalog sub error', ctx.event);
      updateConnState('error', String(ctx.event));
    })
    .subscribe([tables.storeProduct]);
}

async function main() {
  updateConnState('connecting');
  let conn: DbConnection;
  try {
    const config = await loadServerConfig();
    conn = await connect(config);
  } catch (err) {
    console.error('STDB connect failed:', err);
    updateConnState('error', err instanceof Error ? err.message : String(err));
    return;
  }

  registerRowCallbacks(conn);

  window.stdb = {
    getOrCreateCustomer: args => api('/api/customer', args),
    createCheckoutSession: args =>
      api('/api/checkout', {
        items: args.items,
        customerId: args.customerId,
        mode: args.mode,
      }),
    validatePrice: priceId => api('/api/validate-price', { priceId }),
    getWebhookEventCount: () =>
      api<{ count: number }>('/api/webhook-event-count').then(
        result => result.count
      ),
  };

  subscribeToTables(conn);
}

main();
