// Node process: serves the static UI and seeds Resend config on startup. It does
// NOT process webhooks - POST /webhook/resend is a thin passthrough to the module's
// own native HTTP route, which verifies (in-module, via crypto-ts) and ingests.

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { existsSync, readFileSync } from 'node:fs';
import express, { type Request, type Response } from 'express';
import dotenv from 'dotenv';
import { DbConnection, type ErrorContext } from './src/module_bindings';
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

// Shared env supplies secrets/defaults; example-local env wins for app settings.
// Explicit process environment has highest priority.
loadEnv(path.resolve(__dirname, '..', '..', '.env'), false);
loadEnv(path.resolve(__dirname, '..', '.env'), false);
loadEnv(path.resolve(__dirname, '.env'), true);

const PORT = Number.parseInt(process.env.PORT ?? '8790', 10);
const HOST = process.env.HOST?.trim() || '127.0.0.1';
const STDB_URI = process.env.STDB_URI ?? 'ws://127.0.0.1:3000';
const STDB_HTTP = process.env.STDB_HTTP ?? 'http://127.0.0.1:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-resend-example';
const SPACETIME_BIN = process.env.SPACETIME_BIN?.trim() || 'spacetime';
const RESEND_API_KEY = process.env.RESEND_API_KEY ?? '';
const RESEND_WEBHOOK_SECRET = process.env.RESEND_WEBHOOK_SECRET ?? '';
const DEFAULT_FROM = process.env.DEFAULT_FROM ?? 'onboarding@resend.dev';
const RESEND_TEST_RECIPIENTS = [
  'delivered@resend.dev',
  'bounced@resend.dev',
  'complained@resend.dev',
];
const ALLOWED_RECIPIENTS = [
  ...new Set([
    ...RESEND_TEST_RECIPIENTS,
    ...(process.env.RESEND_ALLOWED_RECIPIENTS ?? '')
      .split(',')
      .map(value => value.trim().toLowerCase())
      .filter(Boolean),
  ]),
];
const SERVER_TOKEN_PATH = path.resolve(__dirname, '.stdb-server-token');

let stdb: DbConnection | null = null;
let resendConfigured = false;

type ConnectedServer = {
  connection: DbConnection;
  identity: string;
};

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

function requireStdb(): DbConnection {
  if (!stdb) throw new Error('STDB not connected yet');
  return stdb;
}

const app = express();

// Webhook uses RAW body (svix signs raw bytes); mounted before express.json().
app.post(
  '/webhook/resend',
  express.raw({ type: '*/*', limit: '512kb' }),
  handleResendWebhook
);

app.use(express.json({ limit: '512kb' }));
app.use(express.static(path.join(__dirname, 'public')));

app.get('/api/health', (_req: Request, res: Response) => {
  res.json({ ok: true, database: DB_NAME });
});

app.get('/api/config', (_req: Request, res: Response) => {
  res.json({
    stdbUri: STDB_URI,
    database: DB_NAME,
    resendConfigured,
    defaultFrom: DEFAULT_FROM,
    allowedRecipients: ALLOWED_RECIPIENTS,
  });
});

// Forward the raw body and Svix headers to the module's native route for
// signature verification and ingestion.
async function handleResendWebhook(req: Request, res: Response): Promise<void> {
  const rawBody =
    req.body instanceof Buffer ? req.body : Buffer.from(String(req.body ?? ''));
  const url = `${STDB_HTTP}/v1/database/${DB_NAME}/route/webhook/resend`;

  const headers: Record<string, string> = {
    'content-type': 'application/json',
  };
  for (const name of ['svix-id', 'svix-timestamp', 'svix-signature']) {
    const value = req.headers[name];
    if (typeof value === 'string') headers[name] = value;
    else if (Array.isArray(value)) headers[name] = value.join(',');
  }

  try {
    const upstream = await fetch(url, {
      method: 'POST',
      headers,
      body: rawBody,
    });
    const text = await upstream.text();
    res.status(upstream.status).send(text);
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    console.error(`[webhook] passthrough to STDB route failed: ${reason}`);
    res.status(502).send(`passthrough failed: ${reason}`);
  }
}

async function bootstrapResendConfig(): Promise<void> {
  if (!RESEND_API_KEY) {
    resendConfigured = false;
    process.stdout.write(
      '  ! RESEND_API_KEY missing: Dispatch connects; email sends require configuration\n'
    );
    return;
  }

  await requireStdb().procedures['resend.setResendConfig']({
    apiKey: RESEND_API_KEY,
    webhookSigningSecret: RESEND_WEBHOOK_SECRET || undefined,
    defaultFrom: DEFAULT_FROM,
  });
  await requireStdb().procedures.setDispatchPolicy({
    allowedRecipientsJson: JSON.stringify(ALLOWED_RECIPIENTS),
  });
  resendConfigured = true;
  process.stdout.write('  + Resend config loaded from server environment\n');
}

(async () => {
  console.log(`[stdb] connecting to ${STDB_URI}/${DB_NAME} ...`);
  try {
    const connected = await connectStdb();
    stdb = connected.connection;
    grantServerIdentity({
      spacetimeBin: SPACETIME_BIN,
      server: STDB_HTTP,
      database: DB_NAME,
      procedure: 'resend.add_admin_identity',
      identity: connected.identity,
    });
    console.log(`[stdb] connected as authorized server ${connected.identity}`);
  } catch (err) {
    console.error(
      `[stdb] connection or authorization failed: ${err instanceof Error ? err.message : String(err)}`
    );
    console.error(
      '[stdb] is the SpacetimeDB host running and the module published?'
    );
    process.exit(1);
  }

  try {
    await bootstrapResendConfig();
  } catch (err) {
    console.error(
      `[resend] configuration failed: ${err instanceof Error ? err.message : String(err)}`
    );
    process.exit(1);
  }

  app.listen(PORT, HOST, () => {
    process.stdout.write(`\nDispatch running at http://${HOST}:${PORT}\n`);
    if (!RESEND_WEBHOOK_SECRET) {
      process.stdout.write(
        '  ! RESEND_WEBHOOK_SECRET not set - incoming webhooks are rejected\n'
      );
    }
    process.stdout.write(
      `  webhook endpoint: POST http://127.0.0.1:${PORT}/webhook/resend\n`
    );
    process.stdout.write(`  database: ${STDB_URI}/${DB_NAME}\n\n`);
  });
})();
