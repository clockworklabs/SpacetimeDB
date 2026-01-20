import Database from 'better-sqlite3';
import type {
  Database as BetterSqlite3Database,
  Statement,
} from 'better-sqlite3';
import type { SqlConnector } from '../../core/connectors.ts';
import {
  applySqlitePragmas,
  ensureSqliteDirExists,
  getSqliteMode,
  sqliteModeRank,
} from '../sqlite_common.ts';

export default function sqlite(
  file = process.env.SQLITE_FILE || './.data/sqlite/accounts.sqlite',
): SqlConnector {
  let db: BetterSqlite3Database | undefined;
  const mode = getSqliteMode();
  console.log(`[sqlite] mode=${mode}`);

  const useStmtCache =
    sqliteModeRank(mode) >= sqliteModeRank('realistic_cached');

  const stmtCache = new Map<string, Statement>();

  function assertDb(): BetterSqlite3Database {
    if (!db) throw new Error('SQLite (better-sqlite3) not connected');
    return db;
  }

  function getStmt(sql: string): Statement {
    const d = assertDb();

    if (!useStmtCache) {
      // no caching: always prepare a fresh statement
      return d.prepare(sql);
    }

    const key = sql;
    const cached = stmtCache.get(key);
    if (cached) return cached;

    const stmt = d.prepare(sql);
    stmtCache.set(key, stmt);
    return stmt;
  }

  const root: SqlConnector = {
    name: 'sqlite',

    async open() {
      if (db) return;

      console.log(`[sqlite] opening file=${file} mode=${mode}`);

      await ensureSqliteDirExists(file, mode);
      db = mode === 'fastest' ? new Database(':memory:') : new Database(file);

      applySqlitePragmas(db, mode);

      db.exec(`
        CREATE TABLE IF NOT EXISTS accounts (
          id INTEGER PRIMARY KEY,
          balance INTEGER NOT NULL
        );
      `);

      const row = db.prepare('SELECT COUNT(*) AS c FROM accounts').get() as { c: number };
      console.log('[sqlite] accounts count=', row.c);
    },

    async close() {
      stmtCache.clear();
      if (db) {
        db.close();
        db = undefined;
      }
    },

    async exec(sql: string, params?: unknown[]): Promise<unknown[]> {
      const text = sql.trim();
      const upper = text.toUpperCase();
      const args = (params ?? []) as any[];

      const stmt = getStmt(text);

      const bind: Record<string, any> = {};
      for (let i = 0; i < args.length; i++) {
        bind[String(i + 1)] = args[i];
      }

      if (
        upper.startsWith('BEGIN') ||
        upper.startsWith('COMMIT') ||
        upper.startsWith('ROLLBACK')
      ) {
        stmt.run(bind);
        return [];
      }

      const rows = stmt.all(bind);
      return rows as unknown[];
    },

    async begin() {
      assertDb().exec('BEGIN');
    },

    async commit() {
      assertDb().exec('COMMIT');
    },

    async rollback() {
      assertDb().exec('ROLLBACK');
    },

    async getAccount(id: number) {
      const d = assertDb();
      const stmt = d.prepare('SELECT id, balance FROM accounts WHERE id = ?');
      const row = stmt.get(id) as { id: number; balance: number } | undefined;
      if (!row) return null;

      return {
        id: row.id,
        balance: BigInt(row.balance),
      };
    },

    async verify() {
      const db = assertDb();

      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[sqlite] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      let initial: bigint;
      try {
        initial = BigInt(rawInitial);
      } catch {
        console.error(
          `[sqlite] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
        );
        return;
      }

      const row = db
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
        console.error('[sqlite] verify failed: accounts=0');
        throw new Error('sqlite verification failed: no accounts');
      }

      // 1) total must be conserved
      if (total !== expected) {
        console.error(
          `[sqlite] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
        );
        throw new Error('sqlite verification failed: total_balance mismatch');
      }

      // 2) at least one row must have changed
      if (changed === 0n) {
        console.error(
          `[sqlite] verify failed: all ${count} accounts still at initial balance=${initial}; workload may not have executed`,
        );
        throw new Error('sqlite verification failed: no balances changed');
      }

      console.log(
        `[sqlite] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
      );
    },
  };

  (root as any).createWorker = async (): Promise<SqlConnector> => {
    await root.open(); // ensure db exists once

    const worker: SqlConnector = {
      name: 'sqlite',

      async open() {
        // no-op; root.open() already created the db
      },

      async close() {
        // no-op; root.close() will actually close the db
      },

      exec: root.exec,
      begin: root.begin,
      commit: root.commit,
      rollback: root.rollback,
      getAccount: root.getAccount,

      async verify() {
        throw new Error(
          'sqlite worker connector does not support verify(); call verify() on the root connector instead',
        );
      },
    };

    return worker;
  };

  return root;
}
