import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import { exampleUiAssetsDir } from '@spacetimedb/submodule-shared/server';

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

const PORT = Number.parseInt(process.env.PORT ?? '8799', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-files-example';

const app = express();
app.use(express.json({ limit: '256kb' }));

function proxyStdbRoute(prefix: string) {
  return async (req: Request, res: Response) => {
    const mountedUrl = req.url.startsWith('/?') ? req.url.slice(1) : req.url;
    let fullPath = `${prefix}${mountedUrl}`;
    if (prefix === '/files' && mountedUrl.startsWith('/')) {
      const qIdx = mountedUrl.indexOf('?');
      const rawPath = qIdx < 0 ? mountedUrl : mountedUrl.slice(0, qIdx);
      const originalQuery = qIdx < 0 ? '' : mountedUrl.slice(qIdx + 1);
      const pathQuery = `path=${encodeURIComponent(decodeURIComponent(rawPath))}`;
      fullPath = `/files?${originalQuery ? `${pathQuery}&${originalQuery}` : pathQuery}`;
    }
    const qIdx = fullPath.indexOf('?');
    const routePath = qIdx < 0 ? fullPath : fullPath.slice(0, qIdx);
    const query = qIdx < 0 ? '' : fullPath.slice(qIdx);
    const upstreamUrl = `${STDB_HTTP}/v1/database/${DB_NAME}/route${routePath}${query}`;

    const headers: Record<string, string> = {};
    for (const [key, value] of Object.entries(req.headers)) {
      if (typeof value === 'string') headers[key] = value;
      else if (Array.isArray(value)) headers[key] = value.join(', ');
    }
    delete headers.host;
    delete headers['content-length'];

    try {
      const upstream = await fetch(upstreamUrl, {
        method: req.method,
        headers,
        redirect: 'manual',
      });
      res.status(upstream.status);
      upstream.headers.forEach((value, key) => {
        const lower = key.toLowerCase();
        if (lower === 'transfer-encoding' || lower === 'content-encoding')
          return;
        res.setHeader(key, value);
      });
      if (req.method === 'HEAD') {
        res.end();
        return;
      }
      res.send(Buffer.from(await upstream.arrayBuffer()));
    } catch (err) {
      res.status(502).json({
        error: 'upstream_unreachable',
        detail: (err as Error).message,
      });
    }
  };
}

app.use('/files', proxyStdbRoute('/files'));
app.use('/assets', express.static(exampleUiAssetsDir));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({ spacetimeUri: STDB_URI, databaseName: DB_NAME });
});

app.listen(PORT, HOST, () => {
  console.log(`Vault example running at http://${HOST}:${PORT}`);
  console.log(`  STDB ws  -> ${STDB_URI}`);
  console.log(`  STDB http-> ${STDB_HTTP} (proxy /files/*)`);
  console.log(`  Database -> ${DB_NAME}`);
});
