// Direct Postgres connector: closes the network-hop asymmetry vs the STDB
// connector by skipping the Node RPC server entirely. The bench process
// talks to Postgres directly via a shared pg.Pool (libpq-equivalent in
// node-postgres). Sets maxInflightPerWorker so it can be pipelined like
// the spacetimedb connector.
import 'dotenv/config';
import { Pool, type PoolClient } from 'pg';
import type { RpcConnector } from '../core/connectors.ts';

interface PostgresDirectConfig {
  url: string;
  poolMax: number;
  initialBalance: bigint;
}

let sharedPool: Pool | null = null;
let storedProcInstalled = false;

function getSharedPool(config: PostgresDirectConfig): Pool {
  if (!sharedPool) {
    sharedPool = new Pool({
      connectionString: config.url,
      application_name: 'pg-direct',
      max: config.poolMax,
    });
  }
  return sharedPool;
}

async function ensureSchema(pool: Pool) {
  if (storedProcInstalled) return;
  const client = await pool.connect();
  try {
    await client.query(`
      CREATE TABLE IF NOT EXISTS accounts (
        id INT PRIMARY KEY,
        balance BIGINT NOT NULL
      );
    `);
    await client.query(`
      CREATE TABLE IF NOT EXISTS transfer_audit (
        id BIGSERIAL PRIMARY KEY,
        from_id INT NOT NULL,
        to_id INT NOT NULL,
        amount BIGINT NOT NULL,
        ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
      );
    `);
    // single-call transfer (mirrors postgres-storedproc-rpc-server.ts)
    await client.query(`
      CREATE OR REPLACE FUNCTION do_transfer(
        p_from_id INTEGER, p_to_id INTEGER, p_amount BIGINT
      ) RETURNS VOID AS $$
      DECLARE
        v_from_balance BIGINT;
        v_to_balance   BIGINT;
      BEGIN
        IF p_from_id = p_to_id OR p_amount <= 0 THEN RETURN; END IF;
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
        IF v_from_balance < p_amount THEN RETURN; END IF;
        UPDATE accounts SET balance = balance - p_amount WHERE id = p_from_id;
        UPDATE accounts SET balance = balance + p_amount WHERE id = p_to_id;
      END;
      $$ LANGUAGE plpgsql;
    `);
    // multi-step single-call: read balances, fraud check, transfer, audit
    await client.query(`
      CREATE OR REPLACE FUNCTION do_transfer_with_audit(
        p_from_id INTEGER, p_to_id INTEGER, p_amount BIGINT, p_fraud_limit BIGINT
      ) RETURNS VOID AS $$
      DECLARE
        v_from_balance BIGINT;
        v_to_balance   BIGINT;
      BEGIN
        IF p_from_id = p_to_id OR p_amount <= 0 OR p_amount > p_fraud_limit THEN RETURN; END IF;
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
        IF v_from_balance < p_amount THEN RETURN; END IF;
        UPDATE accounts SET balance = balance - p_amount WHERE id = p_from_id;
        UPDATE accounts SET balance = balance + p_amount WHERE id = p_to_id;
        INSERT INTO transfer_audit (from_id, to_id, amount) VALUES (p_from_id, p_to_id, p_amount);
      END;
      $$ LANGUAGE plpgsql;
    `);
  } finally {
    client.release();
  }
  storedProcInstalled = true;
}

async function txnTransferSteps(
  pool: Pool,
  fromId: number,
  toId: number,
  amount: bigint,
) {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    const lo = fromId < toId ? fromId : toId;
    const hi = fromId < toId ? toId : fromId;
    const sel = await client.query(
      'SELECT id, balance FROM accounts WHERE id IN ($1, $2) ORDER BY id FOR UPDATE',
      [lo, hi],
    );
    if (sel.rows.length !== 2) {
      await client.query('ROLLBACK');
      return;
    }
    const fromRow = sel.rows.find((r: any) => r.id === fromId);
    if (!fromRow || BigInt(fromRow.balance) < amount) {
      await client.query('ROLLBACK');
      return;
    }
    await client.query('UPDATE accounts SET balance = balance - $1 WHERE id = $2', [amount, fromId]);
    await client.query('UPDATE accounts SET balance = balance + $1 WHERE id = $2', [amount, toId]);
    await client.query('COMMIT');
  } catch (err) {
    try { await client.query('ROLLBACK'); } catch {}
    throw err;
  } finally {
    client.release();
  }
}

async function txnTransferWithAuditSteps(
  pool: Pool,
  fromId: number,
  toId: number,
  amount: bigint,
  fraudLimit: bigint,
) {
  if (amount > fraudLimit) return;
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    const lo = fromId < toId ? fromId : toId;
    const hi = fromId < toId ? toId : fromId;
    const sel = await client.query(
      'SELECT id, balance FROM accounts WHERE id IN ($1, $2) ORDER BY id FOR UPDATE',
      [lo, hi],
    );
    if (sel.rows.length !== 2) {
      await client.query('ROLLBACK');
      return;
    }
    const fromRow = sel.rows.find((r: any) => r.id === fromId);
    if (!fromRow || BigInt(fromRow.balance) < amount) {
      await client.query('ROLLBACK');
      return;
    }
    await client.query('UPDATE accounts SET balance = balance - $1 WHERE id = $2', [amount, fromId]);
    await client.query('UPDATE accounts SET balance = balance + $1 WHERE id = $2', [amount, toId]);
    await client.query(
      'INSERT INTO transfer_audit (from_id, to_id, amount) VALUES ($1, $2, $3)',
      [fromId, toId, amount],
    );
    await client.query('COMMIT');
  } catch (err) {
    try { await client.query('ROLLBACK'); } catch {}
    throw err;
  } finally {
    client.release();
  }
}

export default function postgres_direct(
  config: PostgresDirectConfig,
): RpcConnector {
  const root: RpcConnector = {
    name: 'postgres_direct',
    maxInflightPerWorker: 128,

    async open() {
      const pool = getSharedPool(config);
      await ensureSchema(pool);
      // probe
      const c = await pool.connect();
      c.release();
    },

    async close() {
      // shared pool - leave it alive for other workers; cleanup on process exit
    },

    async getAccount(id: number) {
      const pool = getSharedPool(config);
      const r = await pool.query(
        'SELECT id, balance FROM accounts WHERE id = $1 LIMIT 1',
        [id],
      );
      if (r.rows.length === 0) return null;
      const row = r.rows[0]!;
      return { id: row.id, balance: BigInt(row.balance) };
    },

    async verify() {
      const pool = getSharedPool(config);
      const r = await pool.query(`
        SELECT
          COUNT(*)::bigint AS count,
          COALESCE(SUM(balance), 0)::bigint AS total,
          SUM(CASE WHEN balance != $1::bigint THEN 1 ELSE 0 END)::bigint AS changed
        FROM accounts
      `, [config.initialBalance]);
      const row = r.rows[0];
      const count = BigInt(row.count);
      const total = BigInt(row.total);
      const changed = BigInt(row.changed);
      const expected = config.initialBalance * count;
      if (count === 0n) throw new Error('verify failed: accounts=0');
      if (total !== expected) {
        throw new Error(
          `verify failed: accounts=${count} total=${total} expected=${expected}`,
        );
      }
      if (changed === 0n) throw new Error('verify failed: no balances changed');
    },

    async call(name: string, args?: Record<string, unknown>) {
      const pool = getSharedPool(config);
      switch (name) {
        case 'health':
          return { ok: true };
        case 'transfer': {
          const fromId = Number(args?.from_id ?? args?.from);
          const toId = Number(args?.to_id ?? args?.to);
          const amount = BigInt((args?.amount as any) ?? 0);
          if (!Number.isInteger(fromId) || !Number.isInteger(toId)) {
            throw new Error('invalid transfer args');
          }
          if (fromId === toId || amount <= 0n) return;
          await pool.query('SELECT do_transfer($1, $2, $3)', [fromId, toId, amount]);
          return;
        }
        case 'transfer_steps': {
          const fromId = Number(args?.from_id ?? args?.from);
          const toId = Number(args?.to_id ?? args?.to);
          const amount = BigInt((args?.amount as any) ?? 0);
          if (!Number.isInteger(fromId) || !Number.isInteger(toId)) {
            throw new Error('invalid transfer_steps args');
          }
          if (fromId === toId || amount <= 0n) return;
          await txnTransferSteps(pool, fromId, toId, amount);
          return;
        }
        case 'transfer_with_audit': {
          const fromId = Number(args?.from_id ?? args?.from);
          const toId = Number(args?.to_id ?? args?.to);
          const amount = BigInt((args?.amount as any) ?? 0);
          const fraudLimit = BigInt((args?.fraud_limit as any) ?? 1_000_000n);
          if (!Number.isInteger(fromId) || !Number.isInteger(toId)) {
            throw new Error('invalid transfer_with_audit args');
          }
          if (fromId === toId || amount <= 0n) return;
          await pool.query(
            'SELECT do_transfer_with_audit($1, $2, $3, $4)',
            [fromId, toId, amount, fraudLimit],
          );
          return;
        }
        case 'transfer_with_audit_steps': {
          const fromId = Number(args?.from_id ?? args?.from);
          const toId = Number(args?.to_id ?? args?.to);
          const amount = BigInt((args?.amount as any) ?? 0);
          const fraudLimit = BigInt((args?.fraud_limit as any) ?? 1_000_000n);
          if (!Number.isInteger(fromId) || !Number.isInteger(toId)) {
            throw new Error('invalid transfer_with_audit_steps args');
          }
          if (fromId === toId || amount <= 0n) return;
          await txnTransferWithAuditSteps(pool, fromId, toId, amount, fraudLimit);
          return;
        }
        case 'seed': {
          const count = Number(args?.accounts ?? 0);
          const initial = BigInt((args?.initialBalance as any) ?? config.initialBalance);
          if (!Number.isInteger(count) || count <= 0) {
            throw new Error('invalid seed: accounts');
          }
          const c = await pool.connect();
          try {
            await c.query('BEGIN');
            await c.query('DELETE FROM accounts');
            await c.query('DELETE FROM transfer_audit');
            const batch = 10000;
            for (let s = 0; s < count; s += batch) {
              const e = Math.min(s + batch, count);
              const values: string[] = [];
              const params: any[] = [];
              let p = 1;
              for (let id = s; id < e; id++) {
                values.push(`($${p++}, $${p++})`);
                params.push(id, initial);
              }
              await c.query(
                `INSERT INTO accounts (id, balance) VALUES ${values.join(', ')}`,
                params,
              );
            }
            await c.query('COMMIT');
          } catch (err) {
            await c.query('ROLLBACK');
            throw err;
          } finally {
            c.release();
          }
          return { accounts: count };
        }
        default:
          throw new Error(`postgres_direct: unknown call ${name}`);
      }
    },

    async createWorker(): Promise<RpcConnector> {
      const worker: RpcConnector = {
        ...root,
        verify: async () => {
          throw new Error('verify() only on root postgres_direct');
        },
      };
      delete worker.createWorker;
      return worker;
    },
  };
  return root;
}
