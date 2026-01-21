import 'dotenv/config';
import http from 'node:http';
import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import { sqliteTable, integer } from 'drizzle-orm/sqlite-core';
import { eq, inArray } from 'drizzle-orm';
import {
  getSqliteMode,
  applySqlitePragmas,
  ensureSqliteDirExistsSync,
  type SqliteMode,
} from '../connectors/sqlite_common.ts';
import { RpcRequest, RpcResponse } from '../connectors/rpc/rpc_common.ts';

const SQLITE_FILE = process.env.SQLITE_FILE ?? './.data/accounts.sqlite';
const mode: SqliteMode = getSqliteMode();

ensureSqliteDirExistsSync(SQLITE_FILE, mode);

const dbFile = new Database(mode === 'fastest' ? ':memory:' : SQLITE_FILE);
applySqlitePragmas(dbFile, mode);

const accounts = sqliteTable('accounts', {
  id: integer('id').primaryKey(),
  balance: integer('balance').notNull(),
});

const db = drizzle(dbFile, { schema: { accounts } });

function ensureSchema() {
  dbFile
    .prepare(
      `CREATE TABLE IF NOT EXISTS accounts (
                                             id INTEGER PRIMARY KEY,
                                             balance INTEGER NOT NULL
       )`,
    )
    .run();
}

ensureSchema();

async function rpcTransfer(args: Record<string, unknown>) {
  const fromId = Number(args.from_id ?? args.from);
  const toId = Number(args.to_id ?? args.to);
  const amount = Number(args.amount);

  if (
    !Number.isInteger(fromId) ||
    !Number.isInteger(toId) ||
    !Number.isFinite(amount)
  ) {
    throw new Error('invalid transfer args');
  }
  if (fromId === toId || amount <= 0) return;

  const delta = BigInt(amount);

  db.transaction((tx) => {
    const rows = tx
      .select()
      .from(accounts)
      .where(inArray(accounts.id, [fromId, toId]))
      .all();

    if (rows.length !== 2) {
      throw new Error('account_missing');
    }

    const [first, second] = rows;
    const fromRow = first.id === fromId ? first : second;
    const toRow = first.id === fromId ? second : first;

    const fromBal = BigInt(fromRow.balance);
    const toBal = BigInt(toRow.balance);

    if (fromBal < delta) return;

    const newFrom = fromBal - delta;
    const newTo = toBal + delta;

    tx.update(accounts)
      .set({ balance: Number(newFrom) })
      .where(eq(accounts.id, fromId));

    tx.update(accounts)
      .set({ balance: Number(newTo) })
      .where(eq(accounts.id, toId));
  });
}

async function rpcGetAccount(args: Record<string, unknown>) {
  const id = Number(args.id);
  if (!Number.isInteger(id)) throw new Error('invalid id');

  const row = db
    .select()
    .from(accounts)
    .where(eq(accounts.id, id))
    .limit(1)
    .get();

  if (!row) return null;

  return {
    id: row.id,
    balance: BigInt(row.balance).toString(),
  };
}

async function rpcVerify() {
  const rawInitial = process.env.SEED_INITIAL_BALANCE;
  if (!rawInitial) {
    console.warn('[sqlite-rpc] SEED_INITIAL_BALANCE not set; skipping verify');
    return { skipped: true };
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`invalid SEED_INITIAL_BALANCE=${rawInitial}`);
  }

  // reuse the same aggregate pattern as direct sqlite connector
  const row = dbFile
    .prepare(
      `
        SELECT
          COUNT(*) AS count,
          COALESCE(SUM(balance), 0) AS total,
          SUM(CASE WHEN balance != ? THEN 1 ELSE 0 END) AS changed
        FROM accounts
      `,
    )
    .get(initial.toString()) as
    | { count: number; total: number; changed: number }
    | undefined;

  const count = BigInt(row?.count ?? 0);
  const total = BigInt(row?.total ?? 0);
  const changed = BigInt(row?.changed ?? 0);
  const expected = initial * count;

  if (count === 0n) {
    throw new Error('verify failed: accounts=0');
  }
  if (total !== expected) {
    throw new Error(
      `verify failed: accounts=${count} total=${total} expected=${expected}`,
    );
  }
  if (changed === 0n) {
    throw new Error('verify failed: total preserved but no balances changed');
  }

  return {
    accounts: count.toString(),
    total: total.toString(),
    changed: changed.toString(),
  };
}

async function rpcSeed(args: Record<string, unknown>) {
  const count = Number(args.accounts ?? process.env.SEED_ACCOUNTS ?? '0');
  const rawInitial =
    (args.initialBalance as string | number | undefined) ??
    process.env.SEED_INITIAL_BALANCE;

  if (!Number.isInteger(count) || count <= 0) {
    throw new Error('[sqlite-rpc] invalid accounts for seed');
  }
  if (rawInitial === undefined || rawInitial === null) {
    throw new Error('[sqlite-rpc] missing initialBalance for seed');
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`[sqlite-rpc] invalid initialBalance=${rawInitial}`);
  }

  const seedTx = dbFile.transaction(() => {
    dbFile.prepare('DELETE FROM accounts').run();

    const insert = dbFile.prepare(
      'INSERT INTO accounts (id, balance) VALUES (?, ?)',
    );

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      for (let id = start; id < end; id++) {
        insert.run(id, Number(initial));
      }
    }
  });

  seedTx();

  console.log(
    `[sqlite-rpc] seeded accounts: count=${count} initial=${initial.toString()}`,
  );
}

async function handleRpc(body: RpcRequest): Promise<RpcResponse> {
  const name = body?.name;
  const args = body?.args ?? {};

  if (!name) return { ok: false, error: 'missing name' };

  try {
    switch (name) {
      case 'health':
        return { ok: true, result: { status: 'ok' } };
      case 'transfer':
        await rpcTransfer(args);
        return { ok: true };
      case 'getAccount':
        return { ok: true, result: await rpcGetAccount(args) };
      case 'verify':
        return { ok: true, result: await rpcVerify() };
      case 'seed':
        return { ok: true, result: await rpcSeed(args) };
      default:
        return { ok: false, error: `unknown method: ${name}` };
    }
  } catch (err: any) {
    // Log full error details on the server, but return a generic message to the client.
    console.error('Unhandled error in handleRpc:', err);
    return { ok: false, error: 'internal error' };
  }
}

const port = Number(process.env.SQLITE_RPC_PORT ?? 4103);

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host}`);

  if (req.method === 'POST' && url.pathname === '/rpc') {
    let buf = '';
    req.on('data', (chunk) => {
      buf += chunk;
    });
    req.on('end', async () => {
      let body: RpcRequest;
      try {
        body = JSON.parse(buf) as RpcRequest;
      } catch {
        res.statusCode = 400;
        res.setHeader('content-type', 'application/json');
        res.end(JSON.stringify({ ok: false, error: 'invalid json' }));
        return;
      }

      const rsp = await handleRpc(body);
      res.statusCode = rsp.ok ? 200 : 500;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(rsp));
    });
    return;
  }

  if (req.method === 'GET' && url.pathname === '/') {
    res.statusCode = 200;
    res.end('sqlite drizzle rpc server');
    return;
  }

  res.statusCode = 404;
  res.end('not found');
});

server.listen(port, () => {
  console.log(
    `sqlite drizzle rpc server listening on http://localhost:${port}`,
  );
});
