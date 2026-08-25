// Serves the browser bundle and connection settings. The browser connects
// directly to SpacetimeDB.

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
    if (override || process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

// Shared env supplies defaults; example-local env wins for app settings.
// Explicit process environment has highest priority.
loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8788', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_APP_DB = process.env.STDB_APP_DATABASE ?? 'spacetime-cron-example';

const app = express();
app.use(express.json({ limit: '256kb' }));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, app: STDB_APP_DB });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({ stdbUri: STDB_URI, appDatabase: STDB_APP_DB });
});

app.listen(PORT, HOST, () => {
  console.log(`Cron example running at http://${HOST}:${PORT}`);
  console.log(`  SpacetimeDB: ${STDB_URI} (${STDB_APP_DB})`);
});
