import { Pool } from 'pg';
import { drizzle } from 'drizzle-orm/node-postgres';
import { pgTable, integer, bigint as pgBigint } from 'drizzle-orm/pg-core';
import { eq, inArray, sql } from 'drizzle-orm';
import { RpcRequest, RpcResponse } from '../src/connectors/rpc/rpc_common';
// import { poolMaxFromEnv } from '../src/helpers';

const DB_URL = process.env.BUN_PG_URL ?? process.env.PG_URL;
if (!DB_URL) throw new Error('BUN_PG_URL or PG_URL not set');

const pool = new Pool({
  connectionString: DB_URL,
  application_name: 'bun-rpc-drizzle',
  // max: poolMaxFromEnv()
});

const accounts = pgTable('accounts', {
  id: integer('id').primaryKey(),
  balance: pgBigint('balance', { mode: 'bigint' }).notNull(),
});

const db = drizzle(pool, { schema: { accounts } });

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
        await rpcSeed(args);
        return { ok: true };

      default:
        return { ok: false, error: `unknown method: ${name}` };
    }
  } catch (err: any) {
    console.error('[bun] rpc error:', err);
    // Return a generic error message to avoid exposing internal details or stack traces.
    return { ok: false, error: 'internal error' };
  }
}

async function rpcSeed(args: Record<string, unknown>) {
  const count = Number(args.accounts ?? process.env.SEED_ACCOUNTS ?? '0');
  const rawInitial =
    (args.initialBalance as string | number | undefined) ??
    process.env.SEED_INITIAL_BALANCE;

  if (!Number.isInteger(count) || count <= 0) {
    throw new Error('invalid accounts for seed');
  }
  if (rawInitial === undefined || rawInitial === null) {
    throw new Error('missing initialBalance for seed');
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`invalid initialBalance=${rawInitial}`);
  }

  await db.transaction(async (tx) => {
    // Create table if needed
    await tx.execute(sql`
      CREATE TABLE IF NOT EXISTS "accounts" (
        "id" integer PRIMARY KEY,
        "balance" bigint NOT NULL
      )
    `);

    await tx.delete(accounts);

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      const values = [];

      for (let id = start; id < end; id++) {
        values.push({ id, balance: initial });
      }

      await tx.insert(accounts).values(values);
    }
  });

  console.log(
    `[bun] seeded accounts: count=${count} initial=${initial.toString()}`,
  );
}

async function rpcTransfer(args: Record<string, unknown>) {
  const fromId = Number(args.from_id ?? args.from);
  const toId = Number(args.to_id ?? args.to);
  const amount = Number(args.amount);

  if (!Number.isInteger(fromId) || !Number.isInteger(toId) || !Number.isFinite(amount)) {
    throw new Error('invalid transfer args');
  }
  if (fromId === toId || amount <= 0) return;

  const delta = BigInt(amount);

  await db.transaction(async (tx) => {
    // Lock both rows in a deterministic order to avoid deadlocks
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
      return; // not enough funds, do nothing (same as other backends)
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
  const balance = row.balance;
  if (balance == null) {
    throw new Error('account balance is null');
  }

  return {
    id: row.id,
    balance: balance.toString(),
  };
}


async function rpcVerify() {
  const rawInitial = process.env.SEED_INITIAL_BALANCE;
  if (!rawInitial) {
    console.warn('[bun] SEED_INITIAL_BALANCE not set; skipping verification');
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
        COUNT(*)::bigint   AS count,
        COALESCE(SUM(balance), 0)::bigint AS total,
        SUM(
          CASE WHEN balance != ${initial}::bigint THEN 1 ELSE 0 END
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
    throw new Error(
      'verify failed: total preserved but no account balances changed',
    );
  }

  return {
    accounts: count.toString(),
    total: total.toString(),
    changed: changed.toString(),
  };
}

const port = Number(process.env.PORT ?? 4001);

Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url);

    if (req.method === 'POST' && url.pathname === '/rpc') {
      let body: RpcRequest;
      try {
        body = (await req.json()) as RpcRequest;
      } catch {
        return new Response(
          JSON.stringify({ ok: false, error: 'invalid json' }),
          { status: 400, headers: { 'content-type': 'application/json' } },
        );
      }

      const rsp = await handleRpc(body);
      return new Response(JSON.stringify(rsp), {
        status: rsp.ok ? 200 : 500,
        headers: { 'content-type': 'application/json' },
      });
    }

    if (req.method === 'GET' && url.pathname === '/') {
      return new Response('bun drizzle rpc server', { status: 200 });
    }

    return new Response('not found', { status: 404 });
  },
});

console.log(`bun drizzle rpc server listening on http://localhost:${port}`);
