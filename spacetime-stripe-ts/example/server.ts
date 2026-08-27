import path from 'node:path';
import { fileURLToPath } from 'node:url';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import {
  discardStoredServerToken,
  exampleUiAssetsDir,
  grantServerIdentity,
  loadServerToken,
  saveServerToken,
} from '@spacetimedb/example-ui/server';
import {
  DbConnection,
  tables,
  type ErrorContext,
} from './src/module_bindings/app';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// App .env wins. Falls back to spacetime-stripe-ts/.env for package-level defaults.
dotenv.config({ path: path.resolve(__dirname, '.env') });
dotenv.config({ path: path.resolve(__dirname, '..', '.env') });

const PORT = Number.parseInt(process.env.PORT ?? '8787', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-stripe-example';
const NODE_ENV = (process.env.NODE_ENV ?? '').replace(/^['"]|['"]$/g, '');
const IS_PRODUCTION = NODE_ENV === 'production';
const SPACETIME_BIN = process.env.SPACETIME_BIN?.trim() || 'spacetime';
const SYNC_PRICES_ON_START = process.env.STRIPE_SYNC_PRICES === '1';
const IS_LOOPBACK_HOST =
  HOST === '127.0.0.1' || HOST === 'localhost' || HOST === '::1';
const ALLOW_BROWSER_PROVIDER_ACTIONS =
  (!IS_PRODUCTION && IS_LOOPBACK_HOST) ||
  process.env.STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS === '1';
const DEFAULT_RETURN_HOST =
  HOST === '0.0.0.0' || HOST === '::' ? '127.0.0.1' : HOST;
const STRIPE_RETURN_BASE_URL =
  process.env.STRIPE_RETURN_BASE_URL?.trim() ||
  `http://${DEFAULT_RETURN_HOST}:${PORT}`;
const SERVER_TOKEN_PATH = path.resolve(__dirname, '.stdb-server-token');

let stdb: DbConnection | null = null;
let stripeConfigured = false;

type SyncPriceResult = {
  productId: string;
  priceId: string;
  action: 'created' | 'linked' | 'kept';
};

type ConnectedServer = {
  connection: DbConnection;
  identity: string;
};

class RequestError extends Error {
  constructor(
    readonly status: number,
    message: string
  ) {
    super(message);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requiredString(
  value: unknown,
  field: string,
  maxLength: number
): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > maxLength
  ) {
    throw new RequestError(400, `invalid_${field}`);
  }
  return value;
}

function optionalString(
  value: unknown,
  field: string,
  maxLength: number
): string | undefined {
  if (value === undefined || value === null || value === '') return undefined;
  return requiredString(value, field, maxLength);
}

function providerActionsReady(): void {
  if (!ALLOW_BROWSER_PROVIDER_ACTIONS) {
    throw new RequestError(403, 'browser_provider_actions_disabled');
  }
  if (!stripeConfigured) throw new RequestError(503, 'stripe_not_configured');
}

function sendRouteError(route: string, error: unknown, res: Response): void {
  if (error instanceof RequestError) {
    res.status(error.status).json({ error: error.message });
    return;
  }
  console.error(
    `[stripe] ${route} failed: ${error instanceof Error ? error.message : String(error)}`
  );
  res.status(502).json({ error: `${route}_failed` });
}

function returnUrl(flag: 'purchased' | 'canceled'): string {
  const url = new URL('/', STRIPE_RETURN_BASE_URL);
  if (IS_PRODUCTION && url.protocol !== 'https:') {
    throw new RequestError(500, 'stripe_return_url_requires_https');
  }
  url.searchParams.set(flag, '1');
  return url.toString();
}

function connectAttempt(token: string | undefined): Promise<ConnectedServer> {
  return new Promise((resolve, reject) => {
    let builder = DbConnection.builder()
      .withUri(STDB_URI)
      .withDatabaseName(DB_NAME)
      .onConnect((connection, identity, nextToken) => {
        if (!process.env.STDB_SERVER_TOKEN?.trim()) {
          saveServerToken(SERVER_TOKEN_PATH, nextToken);
        }
        resolve({ connection, identity: identity.toHexString() });
      })
      .onDisconnect((_ctx, err) => {
        console.error(
          `[stdb] disconnected: ${err?.message ?? 'unknown'}. exiting`
        );
        process.exit(1);
      })
      .onConnectError((_ctx: ErrorContext, err) => reject(err));
    if (token) builder = builder.withToken(token);
    builder.build();
  });
}

async function connect(): Promise<ConnectedServer> {
  const stored = loadServerToken(
    SERVER_TOKEN_PATH,
    process.env.STDB_SERVER_TOKEN
  );
  try {
    return await connectAttempt(stored.token);
  } catch (error) {
    if (stored.source !== 'file') throw error;
    discardStoredServerToken(SERVER_TOKEN_PATH);
    console.warn(
      '[stdb] stored server token was rejected; creating a new identity'
    );
    return connectAttempt(undefined);
  }
}

function requireStdb(): DbConnection {
  if (!stdb) throw new Error('STDB not connected yet');
  return stdb;
}

async function syncStripePricesFromCatalog(): Promise<SyncPriceResult[]> {
  const raw = await requireStdb().procedures.syncStoreProductsWithStripe({});
  return JSON.parse(raw) as SyncPriceResult[];
}

async function configureStripeFromEnv(): Promise<
  'configured' | 'already-configured'
> {
  const secretKey = process.env.STRIPE_SECRET_KEY?.trim();
  if (!secretKey) {
    throw new Error(
      'STRIPE_SECRET_KEY is not set in spacetime-stripe-ts/example/.env.'
    );
  }

  await requireStdb().procedures.configureStripe({
    secretKey,
    stripeVersion: process.env.STRIPE_VERSION || undefined,
    webhookSigningSecret: process.env.STRIPE_WEBHOOK_SECRET || undefined,
  });
  stripeConfigured = true;
  return 'configured';
}

const app = express();
app.use(express.json({ limit: '512kb' }));
app.use('/assets', express.static(exampleUiAssetsDir, staticOptions()));
app.use(express.static(path.join(__dirname, 'public'), staticOptions()));

function staticOptions() {
  if (IS_PRODUCTION) return {};
  return {
    etag: false,
    maxAge: 0,
    setHeaders: (res: Response) => {
      res.setHeader('Cache-Control', 'no-store');
    },
  };
}

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  const envStripeSecret = process.env.STRIPE_SECRET_KEY?.trim() ?? '';
  res.json({
    spacetimeUri: STDB_URI,
    databaseName: DB_NAME,
    hasStripeSecretKey: envStripeSecret.length > 0,
    stripeConfigured,
    adminEndpointsEnabled: false,
    browserProviderActionsEnabled: ALLOW_BROWSER_PROVIDER_ACTIONS,
    syncPricesOnStart: SYNC_PRICES_ON_START,
  });
});

app.post('/api/customer', async (req: Request, res: Response) => {
  try {
    providerActionsReady();
    if (!isRecord(req.body)) throw new RequestError(400, 'invalid_body');
    const userId = requiredString(req.body.userId, 'user_id', 128);
    const email = optionalString(req.body.email, 'email', 320);
    const name = optionalString(req.body.name, 'name', 200);
    const result = await requireStdb().procedures.getOrCreateStoreCustomer({
      userId,
      email,
      name,
    });
    res.json(result);
  } catch (error) {
    sendRouteError('customer', error, res);
  }
});

app.post('/api/checkout', async (req: Request, res: Response) => {
  try {
    providerActionsReady();
    if (!isRecord(req.body) || !Array.isArray(req.body.items)) {
      throw new RequestError(400, 'invalid_body');
    }
    if (req.body.items.length === 0 || req.body.items.length > 20) {
      throw new RequestError(400, 'invalid_items');
    }

    const mode = requiredString(req.body.mode, 'mode', 32);
    if (mode !== 'payment' && mode !== 'subscription') {
      throw new RequestError(400, 'invalid_mode');
    }
    const customerId = optionalString(req.body.customerId, 'customer_id', 255);
    if (customerId && !/^cus_[A-Za-z0-9]+$/.test(customerId)) {
      throw new RequestError(400, 'invalid_customer_id');
    }

    const catalog = [...requireStdb().db.storeProduct.iter()];
    const items = req.body.items.map((value, index) => {
      if (!isRecord(value))
        throw new RequestError(400, `invalid_item_${index}`);
      const priceId = requiredString(value.priceId, `price_id_${index}`, 255);
      const quantity = value.quantity;
      if (
        typeof quantity !== 'number' ||
        !Number.isSafeInteger(quantity) ||
        quantity < 1 ||
        quantity > 99
      ) {
        throw new RequestError(400, `invalid_quantity_${index}`);
      }
      const product = catalog.find(
        row => row.active && row.stripePriceId === priceId && row.mode === mode
      );
      if (!product)
        throw new RequestError(400, `price_not_in_active_catalog_${index}`);
      return { priceId, quantity: BigInt(quantity) };
    });

    const result = await requireStdb().procedures.createStoreCheckoutSession({
      items,
      customerId,
      mode,
      successUrl: returnUrl('purchased'),
      cancelUrl: returnUrl('canceled'),
      metadataJson: undefined,
      subscriptionMetadataJson: undefined,
      paymentIntentMetadataJson: undefined,
    });
    res.json(result);
  } catch (error) {
    sendRouteError('checkout', error, res);
  }
});

app.post('/api/validate-price', async (req: Request, res: Response) => {
  try {
    providerActionsReady();
    if (!isRecord(req.body)) throw new RequestError(400, 'invalid_body');
    const priceId = requiredString(req.body.priceId, 'price_id', 255);
    const inCatalog = [...requireStdb().db.storeProduct.iter()].some(
      row => row.active && row.stripePriceId === priceId
    );
    if (!inCatalog) throw new RequestError(400, 'price_not_in_active_catalog');
    const result = await requireStdb().procedures.validateStoreStripePrice({
      priceId,
    });
    res.json({
      ...result,
      unitAmount:
        result.unitAmount === undefined ? undefined : Number(result.unitAmount),
    });
  } catch (error) {
    sendRouteError('validate_price', error, res);
  }
});

app.get('/api/webhook-event-count', async (_req: Request, res: Response) => {
  try {
    providerActionsReady();
    const count = await requireStdb().procedures.getStoreWebhookEventCount({});
    res.json({ count: Number(count) });
  } catch (error) {
    sendRouteError('webhook_event_count', error, res);
  }
});

async function seedCatalogIfEmpty(conn: DbConnection): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    let resolved = false;
    conn
      .subscriptionBuilder()
      .onApplied(() => {
        if (resolved) return;
        resolved = true;
        resolve();
      })
      .onError((ctx: ErrorContext) => {
        if (resolved) return;
        resolved = true;
        reject(new Error(`catalog probe failed: ${ctx.event}`));
      })
      .subscribe([tables.storeProduct]);
  });

  const count = conn.db.storeProduct.count();
  if (count === 0n) {
    await conn.reducers.seedDefaultStoreProducts({ force: undefined });
    console.log('Seeded default store products (catalog was empty).');
  }
}

(async () => {
  console.log(`[stdb] connecting to ${STDB_URI} (database=${DB_NAME}) ...`);
  try {
    const connected = await connect();
    stdb = connected.connection;
    grantServerIdentity({
      spacetimeBin: SPACETIME_BIN,
      server: STDB_HTTP,
      database: DB_NAME,
      procedure: 'add_admin_identity',
      identity: connected.identity,
    });
    grantServerIdentity({
      spacetimeBin: SPACETIME_BIN,
      server: STDB_HTTP,
      database: DB_NAME,
      procedure: 'stripe.add_admin_identity',
      identity: connected.identity,
    });
    console.log(`[stdb] connected as authorized server ${connected.identity}`);
  } catch (err) {
    console.error(
      `[stdb] connection failed: ${err instanceof Error ? err.message : String(err)}`
    );
    console.error(
      '[stdb] is the SpacetimeDB host running and the example module published?'
    );
    process.exit(1);
  }

  try {
    await seedCatalogIfEmpty(stdb);
  } catch (err) {
    console.warn(
      `[stdb] could not check/seed catalog: ${err instanceof Error ? err.message : String(err)}`
    );
  }

  if (process.env.STRIPE_SECRET_KEY?.trim()) {
    try {
      await configureStripeFromEnv();
      console.log('[stripe] config loaded from server environment');
      if (SYNC_PRICES_ON_START) {
        const results = await syncStripePricesFromCatalog();
        console.log(`[stripe] synchronized ${results.length} catalog prices`);
      }
    } catch (err) {
      console.error(
        `[stripe] setup failed: ${err instanceof Error ? err.message : String(err)}`
      );
      process.exit(1);
    }
  } else {
    console.warn(
      '[stripe] STRIPE_SECRET_KEY is not set; checkout remains unavailable'
    );
  }

  app.listen(PORT, HOST, () => {
    console.log(`Premium Store example running at http://${HOST}:${PORT}`);
  });
})();
