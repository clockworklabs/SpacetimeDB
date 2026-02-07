// src/init/init_sqlite_seed_in_docker.ts
import 'dotenv/config';
import Database from 'better-sqlite3';
import {
  applySqlitePragmas,
  ensureSqliteDirExists,
  getSqliteMode,
} from '../connectors/sqlite_common.ts';

async function main() {
  const file = process.env.SQLITE_FILE || '/data/accounts.sqlite';
  const rawCount = process.env.SEED_ACCOUNTS ?? '0';
  const rawInitial = process.env.SEED_INITIAL_BALANCE;

  const count = Number(rawCount);
  if (!Number.isInteger(count) || count <= 0) {
    throw new Error(`[sqlite-seed] invalid SEED_ACCOUNTS=${rawCount}`);
  }
  if (rawInitial == null) {
    throw new Error('[sqlite-seed] SEED_INITIAL_BALANCE not set');
  }

  let initial: bigint;
  try {
    initial = BigInt(rawInitial);
  } catch {
    throw new Error(
      `[sqlite-seed] invalid SEED_INITIAL_BALANCE=${String(rawInitial)}`,
    );
  }

  const mode = getSqliteMode();
  console.log(
    `[sqlite-seed] file=${file} accounts=${count} initial=${initial.toString()} mode=${mode}`,
  );

  await ensureSqliteDirExists(file, mode);

  const db = new Database(file);
  try {
    applySqlitePragmas(db, mode);

    db.exec(`
      CREATE TABLE IF NOT EXISTS accounts (
                                            id INTEGER PRIMARY KEY,
                                            balance INTEGER NOT NULL
      );
    `);

    db.exec('DELETE FROM accounts');

    const insert = db.prepare(
      'INSERT INTO accounts (id, balance) VALUES (?, ?)',
    );

    const batchSize = 10_000;
    for (let start = 0; start < count; start += batchSize) {
      const end = Math.min(start + batchSize, count);
      db.transaction(() => {
        for (let id = start; id < end; id++) {
          insert.run(id, initial.toString());
        }
      })();
    }

    const row = db
      .prepare('SELECT COUNT(*) AS n, SUM(balance) AS total FROM accounts')
      .get() as { n: number; total: number | null };

    console.log(
      `[sqlite-seed] done: rows=${row.n} total=${row.total ?? 0} expected=${
        initial * BigInt(count)
      }`,
    );
  } finally {
    db.close();
  }
}

main().catch((err) => {
  console.error(
    '[sqlite-seed] failed: an error occurred during sqlite seeding (see secure logs for details)',
  );
  process.exit(1);
});
