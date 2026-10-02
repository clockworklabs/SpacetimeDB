import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import * as dotenv from 'dotenv';
import { exampleUiAssetsDir } from '@spacetimedb/example-ui/server';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const inheritedEnv = new Set(Object.keys(process.env));

function loadEnv(pathname: string, override: boolean): void {
  if (!existsSync(pathname)) return;
  const parsed = dotenv.parse(readFileSync(pathname)) as Record<string, string>;
  for (const [key, value] of Object.entries(parsed)) {
    if (value.trim() === '') continue;
    if (inheritedEnv.has(key)) continue;
    if (override || process.env[key] === undefined) process.env[key] = value;
  }
}

loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8797', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-lobby-example';

const app = express();
app.use(express.json({ limit: '128kb' }));
app.use('/assets', express.static(exampleUiAssetsDir));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({ spacetimeUri: STDB_URI, databaseName: DB_NAME });
});

app.listen(PORT, HOST, () => {
  process.stdout.write(
    `\nspacetime-lobby-example listening on http://${HOST}:${PORT}\n`
  );
  process.stdout.write(`  database: ${STDB_URI}/${DB_NAME}\n\n`);
});
