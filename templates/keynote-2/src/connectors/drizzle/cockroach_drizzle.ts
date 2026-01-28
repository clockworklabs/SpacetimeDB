import { Pool } from 'pg';
import { drizzle } from 'drizzle-orm/node-postgres';
import { eq, inArray, sql } from 'drizzle-orm';
import type { RpcConnector } from '../../core/connectors.ts';
import { crdbAccounts as accounts } from '../../drizzle/schema.ts';
import { poolMax } from '../../helpers.ts';

type Db = ReturnType<typeof drizzle>;

export function cockroach_drizzle(url = process.env.CRDB_URL!): RpcConnector {
  let pool: Pool | null = null;
  let db: Db | null = null;

  function requireDb(): Db {
    if (!db) throw new Error('[cockroach_drizzle] not connected');
    return db;
  }

  async function open(workers?: number) {
    if (!url) throw new Error('CRDB_URL not set');

    // Only create the pool once; subsequent open() calls are no-ops.
    if (pool) return;

    pool = new Pool({
      connectionString: url,
      application_name: 'rtt-crdb-drizzle',
      max: poolMax(workers, 'MAX_POOL', 1000),
    });

    db = drizzle(pool, { schema: { accounts } });
    await pool.query('SELECT 1');
  }

  async function close() {
    if (pool) {
      await pool.end();
      pool = null;
      db = null;
    }
  }

  async function transfer(args: Record<string, unknown>) {
    const fromId = Number(args.from_id ?? args.from);
    const toId = Number(args.to_id ?? args.to);
    const amount = Number(args.amount);

    if (fromId === toId || amount <= 0) return;

    const delta = BigInt(amount);
    const dbi = requireDb();

    await dbi.transaction(async (tx) => {
      const rows = await tx
        .select()
        .from(accounts)
        .where(inArray(accounts.id, [fromId, toId]))
        .for('update')
        .orderBy(accounts.id);

      if (rows.length !== 2) {
        throw new Error('[cockroach_drizzle] account_missing');
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

  async function getAccount(args: Record<string, unknown>) {
    const id = Number(args.id);
    if (!Number.isInteger(id))
      throw new Error('[cockroach_drizzle] invalid id');

    const dbi = requireDb();
    const rows = await dbi
      .select()
      .from(accounts)
      .where(eq(accounts.id, id))
      .limit(1);

    if (rows.length === 0) return null;
    const row = rows[0]!;
    return {
      id: row.id,
      balance: BigInt(row.balance),
    };
  }

  async function verify() {
    const dbi = requireDb();

    const rawInitial = process.env.SEED_INITIAL_BALANCE;
    if (!rawInitial) {
      console.warn(
        '[cockroach_drizzle] SEED_INITIAL_BALANCE not set; skipping verify',
      );
      return { skipped: true };
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      throw new Error(
        `[cockroach_drizzle] invalid SEED_INITIAL_BALANCE=${rawInitial}`,
      );
    }

    const result = await dbi.execute(
      sql`
        SELECT COUNT(*) ::bigint AS count,
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

    if (count === 0n) {
      throw new Error('[cockroach_drizzle] verify failed: accounts=0');
    }
    if (total !== expected) {
      throw new Error(
        `[cockroach_drizzle] verify failed: accounts=${count} total=${total} expected=${expected}`,
      );
    }
    if (changed === 0n) {
      throw new Error(
        '[cockroach_drizzle] verify failed: total preserved but no balances changed',
      );
    }

    return {
      accounts: count.toString(),
      total: total.toString(),
      changed: changed.toString(),
    };
  }

  async function seed() {
    const dbi = requireDb();

    const count = Number(process.env.SEED_ACCOUNTS);
    const rawInitial = process.env.SEED_INITIAL_BALANCE;
    if (!rawInitial) {
      console.warn(
        '[cockroach_drizzle] SEED_INITIAL_BALANCE not set; skipping verification',
      );
      return;
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      console.error(
        `[cockroach_drizzle] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
      );
      return;
    }
    await dbi.transaction(async (tx) => {
      await tx.execute(sql`DELETE
                           FROM ${accounts}`);

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
      `[cockroach_drizzle] seeded accounts: count=${count} initial=${initial.toString()}`,
    );
  }

  const root: RpcConnector = {
    name: 'cockroach_drizzle',

    async open(workers?: number) {
      await open(workers);
    },

    async close() {
      await close();
    },

    async getAccount(id: number) {
      return getAccount({ id });
    },

    async verify() {
      await verify();
    },

    async call(name: string, args?: Record<string, unknown>): Promise<unknown> {
      const a = args ?? {};
      switch (name) {
        case 'transfer':
          return transfer(a);
        case 'getAccount':
          return getAccount(a);
        case 'verify':
          return verify();
        case 'seed':
          return seed();
        default:
          throw new Error(`[cockroach_drizzle] unknown RPC method: ${name}`);
      }
    },

    async createWorker(): Promise<RpcConnector> {
      await open(); // no-op if already open

      const worker: RpcConnector = {
        name: 'cockroach_drizzle_worker',

        async open() {},

        async call(name: string, args?: Record<string, unknown>) {
          return root.call(name, args);
        },

        async getAccount(id: number) {
          return root.getAccount(id);
        },

        async verify() {
          throw new Error(
            'verify() not supported on cockroach_drizzle worker connector; call verify() on the root connector instead',
          );
        },

        async close() {
          // no-op; root.close() will shut down the pool
        },
      };

      return worker;
    },
  };

  return root;
}
