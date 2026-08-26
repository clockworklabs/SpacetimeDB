import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
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
    if (override || process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

// Shared/root env supplies secrets; example-local env supplies app defaults.
// Blank placeholders in the example .env should not erase shared secrets.
loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8791', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-auth-example';
const AUTH_ISSUER_URL =
  process.env.AUTH_ISSUER_URL ?? `http://localhost:${PORT}`;
const AUTH_BASE_URL = process.env.AUTH_BASE_URL ?? AUTH_ISSUER_URL;
const AUTH_COOKIE_NAME = process.env.AUTH_COOKIE_NAME ?? 'stdb_auth';
const AUTH_SESSION_TTL_SECONDS = Number.parseInt(
  process.env.AUTH_SESSION_TTL_SECONDS ?? `${60 * 60 * 24 * 7}`,
  10
);
if (
  !Number.isInteger(AUTH_SESSION_TTL_SECONDS) ||
  AUTH_SESSION_TTL_SECONDS <= 0
) {
  throw new Error('AUTH_SESSION_TTL_SECONDS must be a positive integer');
}
const GOOGLE_OAUTH_ENABLED = Boolean(
  process.env.GOOGLE_CLIENT_ID?.trim() &&
    process.env.GOOGLE_CLIENT_SECRET?.trim()
);
const GITHUB_OAUTH_ENABLED = Boolean(
  process.env.GITHUB_CLIENT_ID?.trim() &&
    process.env.GITHUB_CLIENT_SECRET?.trim()
);
const STDB_SERVER = process.env.STDB_SERVER ?? STDB_HTTP;
const SPACETIME_BIN = 'spacetime';

function configuredValue(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function configuredPem(value: string | undefined): string | undefined {
  return configuredValue(value)?.replace(/\\n/g, '\n');
}

const opt = (value: string | undefined) =>
  value === undefined ? JSON.stringify([1, []]) : JSON.stringify([0, value]);

function configureAuthFromEnv(): void {
  const args = [
    JSON.stringify(AUTH_ISSUER_URL),
    opt(AUTH_BASE_URL),
    opt(AUTH_COOKIE_NAME),
    JSON.stringify([0, AUTH_SESSION_TTL_SECONDS]),
    opt(configuredPem(process.env.AUTH_ES256_PRIVATE_KEY_PEM)),
    opt(configuredValue(process.env.GOOGLE_CLIENT_ID)),
    opt(configuredValue(process.env.GOOGLE_CLIENT_SECRET)),
    opt(configuredValue(process.env.GITHUB_CLIENT_ID)),
    opt(configuredValue(process.env.GITHUB_CLIENT_SECRET)),
  ];

  const result = spawnSync(
    SPACETIME_BIN,
    ['call', '--server', STDB_SERVER, DB_NAME, 'set_auth_config', ...args],
    { stdio: 'inherit', shell: false }
  );
  if (result.status !== 0) {
    throw new Error(`auth config bootstrap failed (exit ${result.status})`);
  }
}

const app = express();
app.use(express.json({ limit: '256kb' }));

// Register this before the /auth proxy so reset links reach the SPA.
app.get('/auth/password/reset', (_req: Request, res: Response) => {
  res.sendFile(path.join(__dirname, 'public', 'index.html'));
});

// Using app.use as middleware since Express 4's `app.all('/auth/*', ...)` does
// not match nested paths reliably.
app.use('/auth', async (req, res) => {
  const fullPath = `/auth${req.url}`; // req.url here is relative to /auth mount
  const qIdx = fullPath.indexOf('?');
  const path = qIdx < 0 ? fullPath : fullPath.slice(0, qIdx);
  const query = qIdx < 0 ? '' : fullPath.slice(qIdx);
  const upstreamUrl = `${STDB_HTTP}/v1/database/${DB_NAME}/route${path}${query}`;
  const headers: Record<string, string> = {};
  for (const [k, v] of Object.entries(req.headers)) {
    if (typeof v === 'string') headers[k] = v;
    else if (Array.isArray(v)) headers[k] = v.join(', ');
  }
  // Host must point at STDB or some setups 404.
  delete headers.host;
  delete headers['content-length'];
  headers['x-forwarded-proto'] = headers['x-forwarded-proto'] ?? req.protocol;

  // redirect:manual so upstream 302s (e.g. OAuth start) pass through to the browser.
  const init: RequestInit = { method: req.method, headers, redirect: 'manual' };
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    init.body = JSON.stringify(req.body);
    headers['content-type'] = 'application/json';
  }

  try {
    const upstream = await fetch(upstreamUrl, init);
    res.status(upstream.status);
    upstream.headers.forEach((val, key) => {
      const lower = key.toLowerCase();
      if (
        lower === 'transfer-encoding' ||
        lower === 'content-encoding' ||
        lower === 'content-length'
      )
        return;
      res.setHeader(key, val);
    });
    const buf = Buffer.from(await upstream.arrayBuffer());
    res.send(buf);
  } catch (err) {
    res
      .status(502)
      .json({ error: 'upstream_unreachable', detail: (err as Error).message });
  }
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    stdbUri: STDB_URI,
    appDatabase: DB_NAME,
    auth: {
      issuerUrl: AUTH_ISSUER_URL,
      baseUrl: AUTH_BASE_URL,
      cookieName: AUTH_COOKIE_NAME,
      sessionTtlSeconds: AUTH_SESSION_TTL_SECONDS,
      hasEs256PrivateKeyPem: Boolean(
        configuredPem(process.env.AUTH_ES256_PRIVATE_KEY_PEM)
      ),
    },
    oauth: {
      google: GOOGLE_OAUTH_ENABLED,
      github: GITHUB_OAUTH_ENABLED,
    },
  });
});

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, app: DB_NAME });
});

app.use(express.static(path.join(__dirname, 'public')));

try {
  console.log(`[auth] bootstrapping env config via ${SPACETIME_BIN}`);
  configureAuthFromEnv();
  console.log(`[auth] bootstrapped env config issuer=${AUTH_ISSUER_URL}`);
} catch (err) {
  console.error(
    `[auth] env config bootstrap failed: ${err instanceof Error ? err.message : String(err)}`
  );
  console.error(
    '[auth] is the SpacetimeDB host running and the auth example module published?'
  );
  process.exit(1);
}

app.listen(PORT, HOST, () => {
  console.log(`Notes example running at http://${HOST}:${PORT}`);
  console.log(`  STDB ws  -> ${STDB_URI}`);
  console.log(`  STDB http-> ${STDB_HTTP}  (proxying /auth/*)`);
  console.log(`  Database -> ${DB_NAME}`);
});
