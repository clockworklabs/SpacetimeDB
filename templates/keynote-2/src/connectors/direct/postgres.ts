import { Pool, PoolClient } from 'pg';
import type { SqlConnector } from '../../core/connectors.ts';
import { poolMax } from '../../helpers.ts';

export default function postgres(url = process.env.PG_URL!): SqlConnector {
  let pool: Pool | undefined;

  const root: SqlConnector = {
    name: 'postgres',

    async open(workers?: number) {
      if (!url) throw new Error('PG_URL not set');
      if (pool) return; // idempotent

      pool = new Pool({
        connectionString: url,
        application_name: 'rtt-cli',
        max: poolMax(workers, 'MAX_POOL', 1000),
      });

      // Simple connectivity check
      await pool.query('SELECT 1');
    },

    async close() {
      if (pool) {
        await pool.end();
        pool = undefined;
      }
    },

    async exec(sql: string, params?: unknown[]) {
      if (!pool) throw new Error('Postgres pool not initialized');
      const res = await pool.query(sql, params as any[]);
      return res.rows as any;
    },

    async begin() {
      throw new Error(
        'postgres.begin on the root connector is not supported in pooled mode; use worker connectors instead',
      );
    },

    async commit() {
      throw new Error(
        'postgres.commit on the root connector is not supported in pooled mode; use worker connectors instead',
      );
    },

    async rollback() {
      throw new Error(
        'postgres.rollback on the root connector is not supported in pooled mode; use worker connectors instead',
      );
    },

    async getAccount(id: number) {
      if (!pool) throw new Error('Postgres pool not initialized');
      const r = await pool.query(
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
      if (!pool) throw new Error('Postgres pool not initialized');

      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[postgres] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      let initial: bigint;
      try {
        initial = BigInt(rawInitial);
      } catch {
        console.error(
          `[postgres] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
        );
        return;
      }

      const r = await pool.query(
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
        console.error('[postgres] verify failed: accounts=0');
        throw new Error('postgres verification failed: no accounts');
      }

      // 1) total must be conserved
      if (total !== expected) {
        console.error(
          `[postgres] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
        );
        throw new Error('postgres verification failed: total_balance mismatch');
      }

      // 2) at least one row must have changed
      if (changed === 0n) {
        console.error(
          '[postgres] verify failed: total preserved but no balances changed',
        );
        throw new Error(
          'postgres verification failed: no account balances changed',
        );
      }

      console.log(
        `[postgres] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
      );
    },
  };

  root.createWorker = async (): Promise<SqlConnector> => {
    if (!pool) throw new Error('Postgres pool not initialized');

    const client: PoolClient = await pool.connect();

    const exec: SqlConnector['exec'] = async (
      sql: string,
      params?: unknown[],
    ) => {
      const res = await client.query(sql, params as any[]);
      return res.rows as any;
    };

    const workerConnector: SqlConnector = {
      name: 'postgres',

      // No-op: worker is ready once created.
      async open() {},

      async close() {
        client.release();
      },

      exec,

      async begin() {
        await client.query('BEGIN');
      },

      async commit() {
        await client.query('COMMIT');
      },

      async rollback() {
        await client.query('ROLLBACK');
      },

      async getAccount(id: number) {
        const r = await client.query(
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
        // Should never be called; verification is done on the root connector.
        throw new Error(
          'postgres worker connector does not support verify(); call verify() on the root connector',
        );
      },
    };

    return workerConnector;
  };

  return root;
}
