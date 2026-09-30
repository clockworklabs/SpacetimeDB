import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import { exampleUiAssetsDir } from '@spacetimedb/example-ui/server';
import { PRODUCTS, SCENARIOS } from './catalog/catalog';

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
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-posthog-example';
const POSTHOG_HOST = process.env.POSTHOG_HOST ?? 'https://us.i.posthog.com';
const POSTHOG_PROJECT_API_KEY = process.env.POSTHOG_PROJECT_API_KEY ?? '';
const SPACETIME_BIN = process.env.SPACETIME_BIN?.trim() || 'spacetime';

function callSpacetime(procedureName: string, ...args: unknown[]): void {
  const result = spawnSync(
    SPACETIME_BIN,
    [
      'call',
      '--server',
      STDB_HTTP,
      DB_NAME,
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
app.use('/assets', express.static(exampleUiAssetsDir));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    spacetimeUri: STDB_URI,
    databaseName: DB_NAME,
    posthogAppUrl: POSTHOG_PROJECT_API_KEY ? posthogAppUrl() : null,
  });
});

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
  process.stdout.write(`  database: ${STDB_URI}/${DB_NAME}\n\n`);
});
