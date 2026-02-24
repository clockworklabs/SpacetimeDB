import 'dotenv/config';
import http from 'node:http';
import { Pool } from 'pg';
import { RpcRequest, RpcResponse } from '../connectors/rpc/rpc_common.ts';
import { poolMaxFromEnv } from '../helpers.ts';

/**
 * Fair benchmark variant: Postgres RPC server using a stored procedure
 * for the transfer operation instead of Drizzle ORM with multiple round-trips.
 *
 * This eliminates the ORM overhead and reduces the transfer from 5 SQL
 * round-trips (BEGIN + SELECT FOR UPDATE + UPDATE + UPDATE + COMMIT via Drizzle)
 * to a single SQL call (SELECT do_transfer(...)).
 *
 * This is analogous to what SpacetimeDB does: a single reducer call that
 * executes atomically inside the database engine.
 */

const PG_URL = process.env.PG_URL;
if (!PG_URL) {
  throw new Error('PG_URL not set');
}

const pool = new Pool({
  connectionString: PG_URL,
  application_name: 'pg-storedproc-rpc',
  max: poolMaxFromEnv(),
});

async function ensureStoredProcedure() {
  const client = await pool.connect();
  try {
    // Create the stored procedure if it doesn't exist.
    // This does the exact same logic as the Drizzle version but in a single
    // database call with zero ORM overhead.
    await client.query(`
      CREATE OR REPLACE FUNCTION do_transfer(
        p_from_id INTEGER,
        p_to_id   INTEGER,
        p_amount  BIGINT
      ) RETURNS VOID AS $$
      DECLARE
        v_from_balance BIGINT;
        v_to_balance   BIGINT;
      BEGIN
        IF p_from_id = p_to_id OR p_amount <= 0 THEN
          RETURN;
        END IF;

        -- Row-level lock with consistent ordering to avoid deadlocks
        IF p_from_id < p_to_id THEN
          SELECT balance INTO v_from_balance FROM accounts WHERE id = p_from_id FOR UPDATE;
          SELECT balance INTO v_to_balance   FROM accounts WHERE id = p_to_id   FOR UPDATE;
        ELSE
          SELECT balance INTO v_to_balance   FROM accounts WHERE id = p_to_id   FOR UPDATE;
          SELECT balance INTO v_from_balance FROM accounts WHERE id = p_from_id FOR UPDATE;
        END IF;

        IF v_from_balance IS NULL OR v_to_balance IS NULL THEN
          RAISE EXCEPTION 'account_missing';
        END IF;

        IF v_from_balance < p_amount THEN
          RETURN;  -- insufficient funds, silently skip (matches original behavior)
        END IF;

        UPDATE accounts SET balance = balance - p_amount WHERE id = p_from_id;
        UPDATE accounts SET balance = balance + p_amount WHERE id = p_to_id;
      END;
      $$ LANGUAGE plpgsql;
    `);
    console.log('[pg-storedproc-rpc] stored procedure do_transfer() created/updated');
  } finally {
    client.release();
  }
}

async function rpcTransfer(args: Record<string, unknown>) {
  const fromId = Number(args.from_id ?? args.from);
  const toId = Number(args.to_id ?? args.to);

  if (!Number.isInteger(fromId) || !Number.isInteger(toId)) {
    throw new Error('invalid transfer args');
  }

  // Parse amount directly to BigInt to avoid precision loss for large values.
  // Accepts string, number, or bigint input from JSON.
  let amount: bigint;
  try {
    const raw = args.amount;
    amount = typeof raw === 'bigint' ? raw : BigInt(raw as string | number);
  } catch {
    throw new Error('invalid transfer args: amount is not a valid integer');
  }

  if (fromId === toId || amount <= 0n) return;

  // Single database call - no ORM, no multiple round-trips
  await pool.query('SELECT do_transfer($1, $2, $3)', [fromId, toId, amount]);
}

async function rpcGetAccount(args: Record<string, unknown>) {
  const id = Number(args.id);
  if (!Number.isInteger(id)) throw new Error('invalid id');

  const result = await pool.query(
    'SELECT id, balance FROM accounts WHERE id = $1 LIMIT 1',
    [id],
  );

  if (result.rows.length === 0) return null;
  const row = result.rows[0]!;
  return {
    id: row.id,
    balance: row.balance.toString(),
  };
}

async function rpcVerify() {
  const rawInitial = process.env.SEED_INITIAL_BALANCE;
  if (!rawInitial) {
    console.warn('[pg-storedproc-rpc] SEED_INITIAL_BALANCE not set; skipping verify');
    return { skipped: true };
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`invalid SEED_INITIAL_BALANCE=${rawInitial}`);
  }

  const result = await pool.query(`
    SELECT
      COUNT(*)::bigint AS count,
      COALESCE(SUM("balance"), 0)::bigint AS total,
      SUM(
        CASE WHEN "balance" != $1::bigint THEN 1 ELSE 0 END
      )::bigint AS changed
    FROM accounts
  `, [initial]);

  const row = result.rows[0] as {
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
    throw new Error('[pg-storedproc-rpc] invalid accounts for seed');
  }
  if (rawInitial === undefined || rawInitial === null) {
    throw new Error('[pg-storedproc-rpc] missing initialBalance for seed');
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(`[pg-storedproc-rpc] invalid initialBalance=${rawInitial}`);
  }

  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    await client.query('DELETE FROM accounts');

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      const values: string[] = [];
      const params: any[] = [];
      let paramIdx = 1;

      for (let id = start; id < end; id++) {
        values.push(`($${paramIdx++}, $${paramIdx++})`);
        params.push(id, initial);
      }

      await client.query(
        `INSERT INTO accounts (id, balance) VALUES ${values.join(', ')}`,
        params,
      );
    }

    await client.query('COMMIT');
  } catch (err) {
    await client.query('ROLLBACK');
    throw err;
  } finally {
    client.release();
  }

  console.log(
    `[pg-storedproc-rpc] seeded accounts: count=${count} initial=${initial.toString()}`,
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
    console.error('Error while handling RPC request:', err);
    return { ok: false, error: 'internal server error' };
  }
}

const port = Number(process.env.PG_STOREDPROC_RPC_PORT ?? process.env.PG_RPC_PORT ?? 4105);

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
    res.end('pg storedproc rpc server');
    return;
  }

  res.statusCode = 404;
  res.end('not found');
});

// Create stored procedure on startup, then start listening
ensureStoredProcedure().then(() => {
  server.listen(port, () => {
    console.log(`pg storedproc rpc server listening on http://localhost:${port}`);
  });
}).catch((err) => {
  console.error('Failed to create stored procedure:', err);
  process.exit(1);
});
