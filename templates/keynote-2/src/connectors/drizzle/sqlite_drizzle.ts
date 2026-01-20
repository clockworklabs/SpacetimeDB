import Database from 'better-sqlite3';
import type { Database as BetterSqlite3Database } from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import { eq, inArray } from 'drizzle-orm';
import type { RpcConnector } from '../../core/connectors.ts';
import { sqliteAccounts as accounts } from '../../drizzle/schema.ts';
import {
  applySqlitePragmas,
  ensureSqliteDirExists,
  getSqliteMode,
} from '../sqlite_common.ts';

type Db = ReturnType<typeof drizzle>;
const SQLITE_FILE = process.env.SQLITE_FILE || './.data/accounts.sqlite';

type SqliteConnector = RpcConnector & {
  createWorker?(opts: { index: number; total: number }): Promise<RpcConnector>;
};

export default function sqlite_drizzle(
  file: string = SQLITE_FILE,
): SqliteConnector {
  const mode = getSqliteMode();
  console.log(`[sqlite_drizzle] mode=${mode}`);

  let dbFile: BetterSqlite3Database | null = null;
  let db: Db | null = null;

  function assertDb(): Db {
    if (!db) throw new Error('[sqlite_drizzle] not connected');
    return db;
  }

  function assertDbFile(): BetterSqlite3Database {
    if (!dbFile) throw new Error('[sqlite_drizzle] db file not open');
    return dbFile;
  }

  async function openInternal() {
    if (db && dbFile) return; // idempotent

    await ensureSqliteDirExists(file, mode);

    dbFile = mode === 'fastest' ? new Database(':memory:') : new Database(file);

    applySqlitePragmas(dbFile, mode);

    // Same table shape as direct sqlite connector
    dbFile.exec(`
      CREATE TABLE IF NOT EXISTS accounts (
                                            id INTEGER PRIMARY KEY,
                                            balance INTEGER NOT NULL
      );
    `);

    db = drizzle(dbFile, { schema: { accounts } });
  }

  async function closeInternal() {
    if (dbFile) {
      dbFile.close();
      dbFile = null;
    }
    db = null;
  }

  // -------- core RPC methods (transfer / getAccount / verify / seed) --------

  async function transfer(args: Record<string, unknown>) {
    const fromId = Number(args.from_id ?? args.from);
    const toId = Number(args.to_id ?? args.to);
    const amount = Number(args.amount);

    if (fromId === toId || amount <= 0) return;

    const delta = amount;
    const dbi = assertDb();

    dbi.transaction((tx) => {
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

      if (fromRow.balance < delta) {
        return;
      }

      tx.update(accounts)
        .set({ balance: fromRow.balance - delta })
        .where(eq(accounts.id, fromId));

      tx.update(accounts)
        .set({ balance: toRow.balance + delta })
        .where(eq(accounts.id, toId));
    });
  }

  async function getAccount(args: Record<string, unknown>) {
    const id = Number(args.id);
    if (!Number.isInteger(id)) throw new Error('invalid id');

    const dbi = assertDb();

    const row = dbi
      .select()
      .from(accounts)
      .where(eq(accounts.id, id))
      .limit(1)
      .get();

    if (!row) return null;

    return {
      id: row.id,
      balance: BigInt(row.balance),
    };
  }

  async function verify() {
    const dbf = assertDbFile();

    const rawInitial = process.env.SEED_INITIAL_BALANCE;
    if (!rawInitial) {
      console.warn(
        '[sqlite_drizzle] SEED_INITIAL_BALANCE not set; skipping verification',
      );
      return;
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      console.error(
        `[sqlite_drizzle] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
      );
      return;
    }

    // Use the same aggregate query as direct sqlite connector
    const row = dbf
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
      console.error('[sqlite_drizzle] verify failed: accounts=0');
      throw new Error('sqlite_drizzle verification failed: no accounts');
    }

    if (total !== expected) {
      console.error(
        `[sqlite_drizzle] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
      );
      throw new Error(
        'sqlite_drizzle verification failed: total_balance mismatch',
      );
    }

    if (changed === 0n) {
      console.error(
        `[sqlite_drizzle] verify failed: all ${count} accounts still at initial balance=${initial}; workload may not have executed`,
      );
      throw new Error(
        'sqlite_drizzle verification failed: no balances changed',
      );
    }

    console.log(
      `[sqlite_drizzle] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
    );
  }

  async function seed(args: Record<string, unknown>) {
    const dbf = assertDbFile();

    const rawCount = args.accounts ?? process.env.SEED_ACCOUNTS ?? '0';
    const count = Number(rawCount);
    const rawInitial =
      (args.initialBalance as string | number | undefined) ??
      process.env.SEED_INITIAL_BALANCE;

    if (!Number.isInteger(count) || count <= 0) {
      throw new Error('[sqlite_drizzle] invalid accounts for seed');
    }
    if (rawInitial === undefined || rawInitial === null) {
      throw new Error('[sqlite_drizzle] missing initialBalance for seed');
    }

    let initial: bigint;
    try {
      initial = BigInt(rawInitial);
    } catch {
      throw new Error(
        `[sqlite_drizzle] invalid initialBalance=${String(rawInitial)}`,
      );
    }

    // Seed like a realistic app: simple delete + batched inserts
    dbf.exec('DELETE FROM accounts');

    const insert = dbf.prepare(
      'INSERT INTO accounts (id, balance) VALUES (?, ?)',
    );

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      dbf.transaction(() => {
        for (let id = start; id < end; id++) {
          insert.run(id, initial.toString());
        }
      })();
    }

    console.log(
      `[sqlite_drizzle] seeded accounts: count=${count} initial=${initial.toString()}`,
    );
  }

  // ---------- RpcConnector implementation ----------------------------

  const root: SqliteConnector = {
    name: 'sqlite_drizzle',

    async open() {
      await openInternal();
    },

    async close() {
      await closeInternal();
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
          return seed(a);
        default:
          throw new Error(`[sqlite_drizzle] unknown RPC method: ${name}`);
      }
    },

    async createWorker(): Promise<RpcConnector> {
      await root.open();

      const worker: RpcConnector = {
        name: 'sqlite_drizzle_worker',

        async open() {
          // no-op; root.open() already initialized DB
        },

        async close() {
          // no-op; root.close() tears it down
        },

        async call(name: string, args?: Record<string, unknown>) {
          return root.call(name, args);
        },

        async getAccount(id: number) {
          return root.getAccount(id);
        },

        async verify() {
          throw new Error(
            'verify() not supported on sqlite_drizzle worker connector; call verify() on the root connector instead',
          );
        },
      };

      return worker;
    },
  };

  return root;
}
