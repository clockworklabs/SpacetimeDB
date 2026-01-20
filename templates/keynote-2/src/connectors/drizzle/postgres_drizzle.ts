import { Pool } from 'pg';
import { drizzle } from 'drizzle-orm/node-postgres';
import { eq, inArray } from 'drizzle-orm';
import { pgAccounts as accounts } from '../../drizzle/schema.ts';
import { RpcConnector } from '../../core/connectors.ts';
import { poolMax } from '../../helpers.ts';

type Db = ReturnType<typeof drizzle>;

type PostgresConnector = RpcConnector & {
  createWorker?(opts: { index: number; total: number }): Promise<RpcConnector>;
};

export default function postgres_drizzle(
  url = process.env.PG_URL!,
): PostgresConnector {
  let pool: Pool | null = null;
  let db: Db | null = null;

  async function getDb(): Promise<Db> {
    if (!db) throw new Error('[postgres_drizzle] not connected');
    return db;
  }

  async function open(workers?: number) {
    if (pool) return; // already created

    if (!url) throw new Error('PG_URL not set');

    pool = new Pool({
      connectionString: url,
      application_name: 'rtt-pg-drizzle',
      max: poolMax(workers, 'MAX_POOL', 1000),
    });

    db = drizzle(pool, { schema: { accounts } });
  }

  async function close() {
    await pool?.end();
    pool = null;
    db = null;
  }

  async function transfer(args: Record<string, unknown>) {
    const fromId = Number(args.from_id ?? args.from);
    const toId = Number(args.to_id ?? args.to);
    const amount = Number(args.amount);

    if (fromId === toId || amount <= 0) return;

    const delta = BigInt(amount);
    const dbi = await getDb();

    await dbi.transaction(async (tx) => {
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

  async function getAccount(args: Record<string, unknown>) {
    const id = Number(args.id);
    if (!Number.isInteger(id)) throw new Error('invalid id');

    const dbi = await getDb();
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
    const dbi = await getDb();

    const rawInitial = process.env.SEED_INITIAL_BALANCE;
    if (!rawInitial) {
      console.warn(
        '[postgres_drizzle] SEED_INITIAL_BALANCE not set; skipping verify',
      );
      return { skipped: true };
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      throw new Error(
        `[postgres_drizzle] invalid SEED_INITIAL_BALANCE=${rawInitial}`,
      );
    }

    const rows = await dbi.select().from(accounts);
    if (rows.length === 0) {
      throw new Error('[postgres_drizzle] verify failed: accounts=0');
    }

    let total = 0n;
    let changed = 0n;

    for (const row of rows) {
      const bal = BigInt(row.balance);
      total += bal;
      if (bal !== initial) changed++;
    }

    const count = BigInt(rows.length);
    const expected = initial * count;

    if (total !== expected) {
      throw new Error(
        `[postgres_drizzle] verify failed: accounts=${count} total=${total} expected=${expected}`,
      );
    }

    if (changed === 0n) {
      throw new Error(
        '[postgres_drizzle] verify failed: total preserved but no balances changed',
      );
    }

    return {
      accounts: count.toString(),
      total: total.toString(),
      changed: changed.toString(),
    };
  }

  async function seed(args: Record<string, unknown>) {
    const dbi = await getDb();

    const count = Number(args.accounts ?? process.env.SEED_ACCOUNTS ?? '0');
    const rawInitial =
      (args.initialBalance as string | number | undefined) ??
      process.env.SEED_INITIAL_BALANCE;

    if (!Number.isInteger(count) || count <= 0) {
      throw new Error('[postgres_drizzle] invalid accounts for seed');
    }
    if (rawInitial === undefined || rawInitial === null) {
      throw new Error('[postgres_drizzle] missing initialBalance for seed');
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      throw new Error(
        `[postgres_drizzle] invalid initialBalance=${rawInitial}`,
      );
    }

    await dbi.transaction(async (tx) => {
      await tx.delete(accounts);

      const values = [];
      for (let id = 0; id < count; id++) {
        values.push({ id, balance: initial });
      }

      await tx.insert(accounts).values(values);
    });

    console.log(
      `[postgres_drizzle] seeded accounts: count=${count} initial=${initial.toString()}`,
    );
  }

  const root: PostgresConnector = {
    name: 'postgres_drizzle',

    async open() {
      await open();
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
      switch (name) {
        case 'transfer':
          return transfer(args ?? {});
        case 'getAccount':
          return getAccount(args ?? {});
        case 'verify':
          return verify();
        case 'seed':
          return seed(args ?? {});
        default:
          throw new Error(`[postgres_drizzle] unknown RPC method: ${name}`);
      }
    },

    async createWorker(): Promise<RpcConnector> {
      await root.open();

      const worker: RpcConnector = {
        name: 'postgres_drizzle',

        async open() {
          // no-op; uses shared pool/db
        },

        async close() {
          // no-op; root owns the pool
        },

        async getAccount(id: number) {
          return getAccount({ id });
        },

        async verify() {
          throw new Error(
            'verify() not supported on postgres_drizzle worker connector; call verify() on the root connector instead',
          );
        },

        async call(
          name: string,
          args?: Record<string, unknown>,
        ): Promise<unknown> {
          switch (name) {
            case 'transfer':
              return transfer(args ?? {});
            case 'getAccount':
              return getAccount(args ?? {});
            // seed/verify kept on root only
            default:
              throw new Error(
                `[postgres_drizzle worker] unsupported RPC method: ${name}`,
              );
          }
        },
      };

      return worker;
    },
  };

  return root;
}
