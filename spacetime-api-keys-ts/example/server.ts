import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const inheritedEnv = new Set(Object.keys(process.env));

function loadEnv(pathname: string, override: boolean): void {
  if (!existsSync(pathname)) return;
  const parsed = dotenv.parse(readFileSync(pathname));
  for (const [key, value] of Object.entries(parsed)) {
    if (value.trim() === '') continue;
    if (inheritedEnv.has(key)) continue;
    if (override || process.env[key] === undefined) process.env[key] = value;
  }
}

loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8798', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-api-keys-example';
const STDB_SERVER = process.env.STDB_SERVER ?? STDB_HTTP;

function setApiKeysSecret(secret: string | undefined): boolean {
  const arg =
    secret === undefined
      ? JSON.stringify([1, []])
      : JSON.stringify([0, secret]);
  const result = spawnSync(
    'spacetime',
    [
      'call',
      '--server',
      STDB_SERVER,
      DB_NAME,
      'api_keys.set_api_keys_config',
      arg,
      JSON.stringify([1, []]),
    ],
    { stdio: 'inherit', shell: false }
  );
  return result.status === 0;
}

// The secret keys every API key the module mints, so it is generated here
// rather than inside the module. Without API_KEYS_SECRET the database keeps
// the secret it already stores; after a fresh publish a new one is generated.
function configureApiKeysSecret(): void {
  const secret = process.env.API_KEYS_SECRET?.trim() || undefined;
  if (setApiKeysSecret(secret)) return;
  if (!secret) {
    console.log('[api-keys] no stored secret; generating one');
    if (setApiKeysSecret(randomBytes(32).toString('hex'))) return;
  }
  throw new Error('api-keys secret bootstrap failed');
}

const app = express();
app.use(express.json({ limit: '256kb' }));

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    spacetimeUri: STDB_URI,
    databaseName: DB_NAME,
  });
});

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

// Only the module's colony routes are forwarded. The action is matched
// against this list, so no request path reaches another upstream endpoint.
const COLONY_ROUTES = new Set([
  'snapshot',
  'terraform',
  'build',
  'unbuild',
  'plant',
  'clear',
]);

app.all('/api/colony/:action', async (req: Request, res: Response) => {
  const { action } = req.params;
  if (!COLONY_ROUTES.has(action)) {
    res.status(404).json({ ok: false, error: 'not_found' });
    return;
  }
  const upstreamUrl = `${STDB_HTTP}/v1/database/${DB_NAME}/route/api/colony/${action}`;
  const headers: Record<string, string> = {};
  if (req.headers.authorization) {
    headers.authorization = req.headers.authorization;
  }

  const init: RequestInit = {
    method: req.method,
    headers,
    redirect: 'manual',
  };
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    headers['content-type'] = 'application/json';
    init.body = JSON.stringify(req.body ?? {});
  }

  try {
    const upstream = await fetch(upstreamUrl, init);
    res.status(upstream.status);
    upstream.headers.forEach((value, key) => {
      const lower = key.toLowerCase();
      if (
        lower === 'transfer-encoding' ||
        lower === 'content-encoding' ||
        lower === 'content-length'
      )
        return;
      res.setHeader(key, value);
    });
    res.send(Buffer.from(await upstream.arrayBuffer()));
  } catch (err) {
    res.status(502).json({
      ok: false,
      error: 'stdb_route_unreachable',
      detail: err instanceof Error ? err.message : String(err),
    });
  }
});

app.use(express.static(path.join(__dirname, 'public')));

try {
  configureApiKeysSecret();
} catch (err) {
  console.error(
    `[api-keys] ${err instanceof Error ? err.message : String(err)}; is the SpacetimeDB host running and the Colony module published?`
  );
  process.exit(1);
}

app.listen(PORT, HOST, () => {
  console.log(`Colony running at http://${HOST}:${PORT}`);
  console.log(`  STDB ws  -> ${STDB_URI}`);
  console.log(`  STDB http-> ${STDB_HTTP}`);
  console.log(`  Database -> ${DB_NAME}`);
});
