import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';
import { ACC, BAL } from './utils.ts';

export function initSqlite(file: string) {
  console.log(`\n[sqlite] init @ ${file}`);

  try {
    fs.mkdirSync(path.dirname(path.resolve(file)), { recursive: true });

    const db = new Database(file);

    db.exec(`BEGIN TRANSACTION;`);

    db.exec(`
      CREATE TABLE IF NOT EXISTS accounts(
        id INTEGER PRIMARY KEY,
        balance INTEGER NOT NULL
      );

      -- 🔥 Hard reset
      DELETE FROM accounts;
      
      -- Efficiently insert test data
      WITH RECURSIVE c(x) AS (
        SELECT 0
        UNION ALL
        SELECT x + 1 FROM c LIMIT ${ACC}
      )
      INSERT INTO accounts(id, balance)
      SELECT x, ${BAL} FROM c;
    `);

    db.exec(`COMMIT;`);
    db.close();

    console.log(`[sqlite] ready`);
  } catch (error) {
    console.error(
      `[sqlite] ERROR during initialization for file: ${file}`,
      error,
    );
    throw error;
  }
}
