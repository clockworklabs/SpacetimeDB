import path from 'node:path';
import { fileURLToPath } from 'node:url';
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
const STDB_DATABASE =
  process.env.STDB_DATABASE ??
  process.env.STDB_APP_DATABASE ??
  'spacetime-api-keys-example';

const app = express();
app.use(express.json({ limit: '256kb' }));

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    stdbUri: STDB_URI,
    database: STDB_DATABASE,
  });
});

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, database: STDB_DATABASE });
});

app.use('/api/colony', async (req: Request, res: Response) => {
  const fullPath = `/api/colony${req.url}`;
  const qIdx = fullPath.indexOf('?');
  const subpath = qIdx < 0 ? fullPath : fullPath.slice(0, qIdx);
  const query = qIdx < 0 ? '' : fullPath.slice(qIdx);
  const upstreamUrl = `${STDB_HTTP}/v1/database/${STDB_DATABASE}/route${subpath}${query}`;
  const headers: Record<string, string> = {};
  for (const [key, value] of Object.entries(req.headers)) {
    if (typeof value === 'string') headers[key] = value;
    else if (Array.isArray(value)) headers[key] = value.join(', ');
  }
  delete headers.host;
  delete headers['content-length'];

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

app.listen(PORT, HOST, () => {
  console.log(`Colony running at http://${HOST}:${PORT}`);
  console.log(`  STDB ws  -> ${STDB_URI}`);
  console.log(`  STDB http-> ${STDB_HTTP}`);
  console.log(`  Database -> ${STDB_DATABASE}`);
});
