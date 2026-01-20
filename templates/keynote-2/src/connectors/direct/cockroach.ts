import { Pool, PoolClient } from 'pg';
import type { SqlConnector } from '../../core/connectors.ts';
import { poolMax } from '../../helpers.ts';

export default function cockroach(url = process.env.CRDB_URL!): SqlConnector {
  let pool: Pool | undefined;

  async function ensurePool(workers?: number): Promise<Pool> {
    if (!url) throw new Error('CRDB_URL not set');
    if (!pool) {
      pool = new Pool({
        connectionString: url,
        application_name: 'rtt-cli',
        max: poolMax(workers, 'MAX_POOL', 1000),
      });
    }
    return pool;
  }

  async function rootExec(sql: string, params?: unknown[]) {
    const p = await ensurePool();
    const res = await p.query(sql, params as any[]);
    return res.rows as any;
  }

  const root: SqlConnector = {
    name: 'cockroach',

    async open(workers?: number) {
      const p = await ensurePool(workers);
      const client = await p.connect();
      try {
        await client.query('SELECT 1');
      } finally {
        client.release();
      }
    },

    async close() {
      if (pool) {
        await pool.end();
        pool = undefined;
      }
    },

    exec: rootExec,

    async begin() {
      throw new Error(
        'Transactions not supported on root cockroach connector; use worker connections (createWorker) instead',
      );
    },

    async commit() {
      throw new Error(
        'Transactions not supported on root cockroach connector; use worker connections (createWorker) instead',
      );
    },

    async rollback() {
      throw new Error(
        'Transactions not supported on root cockroach connector; use worker connections (createWorker) instead',
      );
    },

    async getAccount(id: number) {
      const rows = await rootExec(
        'SELECT id, balance FROM accounts WHERE id = $1',
        [id],
      );
      if (!rows || rows.length === 0) return null;
      return {
        id: Number(rows[0].id),
        balance: BigInt(rows[0].balance),
      };
    },

    async verify() {
      const p = await ensurePool();

      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[cockroach] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      let initial: bigint;
      try {
        initial = BigInt(rawInitial);
      } catch {
        console.error(
          `[cockroach] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
        );
        return;
      }

      const r = await p.query(
        `
        SELECT
          COUNT(*)::bigint AS count,
          COALESCE(SUM(balance), 0)::bigint AS total,
          SUM(CASE WHEN balance != $1::bigint THEN 1 ELSE 0 END)::bigint AS changed
        FROM accounts
      `,
        [initial.toString()],
      );

      const row = r.rows[0] as {
        count: string | number | bigint;
        total: string | number | bigint;
        changed: string | number | bigint;
      };

      const count = BigInt(row.count);
      const total = BigInt(row.total);
      const changed = BigInt(row.changed);
      const expected = initial * count;

      if (count === 0n) {
        console.error('[cockroach] verify failed: accounts=0');
        throw new Error('cockroach verification failed: no accounts');
      }

      if (total !== expected) {
        console.error(
          `[cockroach] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
        );
        throw new Error(
          'cockroach verification failed: total_balance mismatch',
        );
      }

      if (changed === 0n) {
        console.error(
          `[cockroach] verify failed: all ${count} accounts still at initial balance=${initial}; workload may not have executed`,
        );
        throw new Error('cockroach verification failed: no balances changed');
      }

      console.log(
        `[cockroach] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
      );
    },

    async createWorker() {
      const p = await ensurePool();

      // This worker keeps track of a client only for the lifetime of a txn.
      let client: PoolClient | null = null;

      async function ensureClient(): Promise<PoolClient> {
        if (!client) {
          client = await p.connect();
        }
        return client;
      }

      const worker: SqlConnector = {
        name: 'cockroach',

        async open() {
          // no-op; lazy client fetch in ensureClient()
        },

        async close() {
          // If a client is currently held (e.g. txn never committed/rolled back),
          // release it.
          if (client) {
            try {
              client.release();
            } catch {}
            client = null;
          }
        },

        async exec(sql: string, params?: unknown[]) {
          const c = await ensureClient();
          const res = await c.query(sql, params as any[]);
          return res.rows as any;
        },

        async begin() {
          const c = await ensureClient();
          await c.query('BEGIN');
        },

        async commit() {
          if (!client) return;
          await client.query('COMMIT');
          client.release();
          client = null; // free the connection back to the pool
        },

        async rollback() {
          if (!client) return;
          await client.query('ROLLBACK');
          client.release();
          client = null;
        },

        async getAccount(id: number) {
          const c = await ensureClient();
          const r = await c.query(
            'SELECT id, balance FROM accounts WHERE id = $1',
            [id],
          );
          if (r.rows.length === 0) return null;
          return {
            id: Number(r.rows[0].id),
            balance: BigInt(r.rows[0].balance),
          };
        },

        async verify() {
          throw new Error(
            'verify() not supported on cockroach worker; call verify() on the root connector instead',
          );
        },
      };

      return worker;
    },
  };

  return root;
}
