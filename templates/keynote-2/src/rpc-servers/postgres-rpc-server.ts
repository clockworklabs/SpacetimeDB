import 'dotenv/config';
import http from 'node:http';
import { Pool } from 'pg';
import { drizzle } from 'drizzle-orm/node-postgres';
import { pgTable, integer, bigint as pgBigint } from 'drizzle-orm/pg-core';
import { eq, inArray, sql } from 'drizzle-orm';
import { RpcRequest, RpcResponse } from '../connectors/rpc/rpc_common.ts';
import { poolMaxFromEnv } from '../helpers.ts';

const PG_URL = process.env.PG_URL;
if (!PG_URL) {
  throw new Error('PG_URL not set');
}

const accounts = pgTable('accounts', {
  id: integer('id').primaryKey(),
  balance: pgBigint('balance', { mode: 'bigint' }).notNull(),
});

const pool = new Pool({
  connectionString: PG_URL,
  application_name: 'pg-rpc-drizzle',
  max: poolMaxFromEnv(),
});

const db = drizzle(pool, { schema: { accounts } });

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

  await db.transaction(async (tx) => {
    const rows = await tx
      .select()
      .from(accounts)
      .where(inArray(accounts.id, [fromId, toId]))
      .for('update')
      .orderBy(accounts.id);

    if (rows.length !== 2) {
      throw new Error('account_missing');
    }

    const [first, second] = rows;
    const fromRow = first.id === fromId ? first : second;
    const toRow = first.id === fromId ? second : first;

    if (fromRow.balance < delta) {
      return;
    }

    await tx
      .update(accounts)
      .set({ balance: fromRow.balance - delta })
      .where(eq(accounts.id, fromId));

    await tx
      .update(accounts)
      .set({ balance: toRow.balance + delta })
      .where(eq(accounts.id, toId));
  });
}

async function rpcGetAccount(args: Record<string, unknown>) {
  const id = Number(args.id);
  if (!Number.isInteger(id)) throw new Error('invalid id');

  const rows = await db
    .select()
    .from(accounts)
    .where(eq(accounts.id, id))
    .limit(1);

  if (rows.length === 0) return null;
  const row = rows[0]!;
  return {
    id: row.id,
    balance: row.balance.toString(),
  };
}

async function rpcVerify() {
  const rawInitial = process.env.SEED_INITIAL_BALANCE;
  if (!rawInitial) {
    console.warn('[pg-rpc] SEED_INITIAL_BALANCE not set; skipping verify');
    return { skipped: true };
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`invalid SEED_INITIAL_BALANCE=${rawInitial}`);
  }

  const result = await db.execute(
    sql`
      SELECT
        COUNT(*)::bigint AS count,
        COALESCE(SUM("balance"), 0)::bigint AS total,
        SUM(
          CASE WHEN "balance" != ${initial}::bigint THEN 1 ELSE 0 END
        )::bigint AS changed
      FROM ${accounts}
    `,
  );

  const row = (result as any).rows[0] as {
    count: string | number | bigint;
    total: string | number | bigint;
    changed: string | number | bigint;
  };

  const count = BigInt(row.count);
  const total = BigInt(row.total);
  const changed = BigInt(row.changed);
  const expected = initial * count;

  if (count === 0n) throw new Error('verify failed: accounts=0');
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
    throw new Error('[pg-rpc] invalid accounts for seed');
  }
  if (rawInitial === undefined || rawInitial === null) {
    throw new Error('[pg-rpc] missing initialBalance for seed');
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`[pg-rpc] invalid initialBalance=${rawInitial}`);
  }

  await db.transaction(async (tx) => {
    await tx.execute(sql`DELETE FROM ${accounts}`);

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      const values: { id: number; balance: bigint }[] = [];

      for (let id = start; id < end; id++) {
        values.push({ id, balance: initial });
      }

      await tx.insert(accounts).values(values as any);
    }
  });

  console.log(
    `[pg-rpc] seeded accounts: count=${count} initial=${initial.toString()}`,
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
    return { ok: false, error: String(err?.message ?? err) };
  }
}

const port = Number(process.env.PG_RPC_PORT ?? 4101);

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
    res.end('pg drizzle rpc server');
    return;
  }

  res.statusCode = 404;
  res.end('not found');
});

server.listen(port, () => {
  console.log(`pg drizzle rpc server listening on http://localhost:${port}`);
});
