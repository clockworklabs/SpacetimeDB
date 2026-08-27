import path from 'node:path';
import { fileURLToPath } from 'node:url';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import { exampleUiAssetsDir } from '@spacetimedb/submodule-shared/server';

dotenv.config();

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PORT = Number.parseInt(process.env.PORT ?? '8792', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const DB_NAME =
  process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-rate-limit-example';

const app = express();
app.use(express.json({ limit: '256kb' }));
app.use('/assets', express.static(exampleUiAssetsDir));
app.use(
  express.static(path.join(__dirname, 'public'), {
    etag: false,
    lastModified: false,
    setHeaders(res) {
      res.setHeader('Cache-Control', 'no-store');
    },
  })
);

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, databaseName: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({ spacetimeUri: STDB_URI, databaseName: DB_NAME });
});

app.listen(PORT, HOST, () => {
  console.log(`Rate-limit example running at http://${HOST}:${PORT}`);
  console.log(`  STDB -> ${STDB_URI} (${DB_NAME})`);
});
