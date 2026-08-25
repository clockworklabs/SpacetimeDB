import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import { DbConnection, tables, type ErrorContext } from './src/module_bindings';
import { PRODUCTS, SCENARIOS } from './catalog/catalog';
import {
  discardStoredServerToken,
  grantServerIdentity,
  loadServerToken,
  saveServerToken,
} from '../../tools/example-server-identity';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const inheritedEnv = new Set(Object.keys(process.env));

function loadEnv(pathname: string, override: boolean): void {
  if (!existsSync(pathname)) return;
  const parsed = dotenv.parse(readFileSync(pathname));
  for (const [key, value] of Object.entries(parsed)) {
    if (value.trim() === '') continue;
    if (inheritedEnv.has(key)) continue;
    if (override || process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8796', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const STDB_DB = process.env.STDB_DATABASE ?? 'spacetime-posthog-example';
const POSTHOG_HOST = process.env.POSTHOG_HOST ?? 'https://us.i.posthog.com';
const POSTHOG_PROJECT_API_KEY = process.env.POSTHOG_PROJECT_API_KEY ?? '';
const SPACETIME_BIN = process.env.SPACETIME_BIN?.trim() || 'spacetime';
const SERVER_TOKEN_PATH = path.resolve(__dirname, '.stdb-server-token');

let stdb: DbConnection | null = null;
let flushTimer: ReturnType<typeof setTimeout> | undefined;
let flushDueAt = 0;
let flushing = false;

type ConnectedServer = {
  connection: DbConnection;
  identity: string;
};

function connectAttempt(token: string | undefined): Promise<ConnectedServer> {
  return new Promise((resolve, reject) => {
    let builder = DbConnection.builder()
      .withUri(STDB_URI)
      .withDatabaseName(STDB_DB)
      .onConnect((connection, identity, nextToken) => {
        if (!process.env.STDB_SERVER_TOKEN?.trim()) {
          saveServerToken(SERVER_TOKEN_PATH, nextToken);
        }
        resolve({ connection, identity: identity.toHexString() });
      })
      .onDisconnect((_ctx, err) => {
        console.error(
          `[stdb] disconnected: ${err?.message ?? 'unknown'} - exiting for supervisor restart`
        );
        process.exit(1);
      })
      .onConnectError((_ctx: ErrorContext, err) => reject(err));
    if (token) builder = builder.withToken(token);
    builder.build();
  });
}

async function connectStdb(): Promise<ConnectedServer> {
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

function callSpacetime(procedureName: string, ...args: unknown[]): void {
  const result = spawnSync(
    SPACETIME_BIN,
    [
      'call',
      '--server',
      STDB_HTTP,
      STDB_DB,
      procedureName,
      ...args.map(arg => JSON.stringify(arg)),
    ],
    { encoding: 'utf8', shell: false }
  );
  if (result.status !== 0) {
    throw new Error(
      result.stderr.trim() ||
        result.stdout.trim() ||
        `spacetime exited ${result.status}`
    );
  }
}

function configurePostHogFromEnv(): void {
  if (!POSTHOG_PROJECT_API_KEY) {
    throw new Error("POSTHOG_PROJECT_API_KEY not set in this server's .env.");
  }
  callSpacetime(
    'posthog.set_posthog_config',
    POSTHOG_HOST,
    POSTHOG_PROJECT_API_KEY
  );
}

function syncCatalog(): void {
  callSpacetime(
    'sync_catalog',
    JSON.stringify(PRODUCTS),
    JSON.stringify(SCENARIOS)
  );
}

function scheduleAnalyticsFlush(): void {
  if (!stdb || flushing) return;
  let nextAttemptMs = Number.POSITIVE_INFINITY;
  for (const row of stdb.db.posthogOutboxAdmin.iter()) {
    const value =
      row.status.tag === 'Queued'
        ? Number(row.nextAttemptAt.microsSinceUnixEpoch / 1000n)
        : row.status.tag === 'Processing'
          ? Number(row.claimExpiresAtMicros / 1000n)
          : Number.POSITIVE_INFINITY;
    if (value < nextAttemptMs) nextAttemptMs = value;
  }
  if (!Number.isFinite(nextAttemptMs)) return;
  const delay = Math.max(0, nextAttemptMs - Date.now());
  const dueAt = Date.now() + delay;
  if (flushTimer && dueAt >= flushDueAt - 5) return;
  if (flushTimer) clearTimeout(flushTimer);
  flushDueAt = dueAt;
  flushTimer = setTimeout(() => {
    flushTimer = undefined;
    flushDueAt = 0;
    void flushAnalytics();
  }, delay);
}

async function flushAnalytics(): Promise<void> {
  if (!stdb || flushing) return;
  flushing = true;
  try {
    await stdb.procedures.flushAnalytics({ limit: 50 });
  } catch (error) {
    console.error(
      `[posthog] delivery failed: ${error instanceof Error ? error.message : String(error)}`
    );
  } finally {
    flushing = false;
    scheduleAnalyticsFlush();
  }
}

function startAnalyticsDelivery(connection: DbConnection): void {
  connection.db.posthogOutboxAdmin.onInsert(scheduleAnalyticsFlush);
  connection.db.posthogOutboxAdmin.onUpdate(scheduleAnalyticsFlush);
  connection
    .subscriptionBuilder()
    .onApplied(scheduleAnalyticsFlush)
    .onError(ctx =>
      console.error(`[posthog] outbox subscription failed: ${ctx.event}`)
    )
    .subscribe([tables.posthogOutboxAdmin]);
}

// Derive the PostHog app (dashboard) URL from the ingestion host, e.g.
// https://us.i.posthog.com -> https://us.posthog.com. Self-hosted hosts are
// already the app host, so they pass through unchanged.
function posthogAppUrl(): string {
  try {
    const u = new URL(POSTHOG_HOST);
    const host = u.hostname.endsWith('.i.posthog.com')
      ? u.hostname.replace('.i.posthog.com', '.posthog.com')
      : u.hostname;
    return `${u.protocol}//${host}${u.port ? `:${u.port}` : ''}`;
  } catch {
    return 'https://us.posthog.com';
  }
}

const app = express();
app.use(express.json({ limit: '256kb' }));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, database: STDB_DB });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    stdbUri: STDB_URI,
    database: STDB_DB,
    posthogAppUrl: POSTHOG_PROJECT_API_KEY ? posthogAppUrl() : null,
  });
});

(async () => {
  console.log(`[stdb] connecting to ${STDB_URI}/${STDB_DB} ...`);
  try {
    const connected = await connectStdb();
    stdb = connected.connection;
    grantServerIdentity({
      spacetimeBin: SPACETIME_BIN,
      server: STDB_HTTP,
      database: STDB_DB,
      procedure: 'posthog.add_admin_identity',
      identity: connected.identity,
    });
    console.log(`[stdb] connected as authorized server ${connected.identity}`);
  } catch (err) {
    console.error(
      `[stdb] connection failed: ${err instanceof Error ? err.message : String(err)}`
    );
    console.error(
      '[stdb] is the SpacetimeDB host running and the module published?'
    );
    process.exit(1);
  }

  try {
    syncCatalog();
    console.log('[catalog] Context Cafe catalog synced');
  } catch (err) {
    console.warn(
      `[catalog] sync failed: ${err instanceof Error ? err.message : String(err)}`
    );
  }

  if (POSTHOG_PROJECT_API_KEY) {
    try {
      configurePostHogFromEnv();
      console.log('[posthog] config loaded from .env');
    } catch (err) {
      console.warn(
        `[posthog] automatic config failed: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  startAnalyticsDelivery(stdb);

  app.listen(PORT, HOST, () => {
    process.stdout.write(
      `\nspacetime-posthog-example listening on http://${HOST}:${PORT}\n`
    );
    if (!POSTHOG_PROJECT_API_KEY) {
      process.stdout.write(
        '  ! POSTHOG_PROJECT_API_KEY not set - configure PostHog in .env and restart\n'
      );
    }
    process.stdout.write(`  spacetime: ${SPACETIME_BIN}\n`);
    process.stdout.write(`  database: ${STDB_URI}/${STDB_DB}\n\n`);
  });
})();
