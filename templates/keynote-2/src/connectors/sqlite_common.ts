import type { Database as BetterSqlite3Database } from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

export type SqliteMode =
  | 'fastest'
  | 'realistic_cached'
  | 'realistic_fast'
  | 'realistic'
  | 'default'
  | 'baseline';

export function getSqliteMode(): SqliteMode {
  switch ((process.env.SQLITE_MODE ?? 'realistic_cached').toLowerCase()) {
    case 'fastest':
      return 'fastest';
    case 'realistic_cached':
      return 'realistic_cached';
    case 'realistic_fast':
      return 'realistic_fast';
    case 'realistic':
      return 'realistic';
    case 'default':
      return 'default';
    case 'baseline':
      return 'baseline';
    default:
      return 'realistic_cached';
  }
}

export function applySqlitePragmas(
  db: BetterSqlite3Database,
  mode: SqliteMode,
): void {
  switch (mode) {
    case 'fastest':
      db.pragma('journal_mode = MEMORY');
      db.pragma('synchronous = OFF');
      db.pragma('temp_store = MEMORY');
      db.pragma('cache_size = -64000');
      db.pragma('foreign_keys = OFF');
      break;
    case 'realistic_cached':
      db.pragma('journal_mode = WAL');
      db.pragma('synchronous = MEMORY');
      db.pragma('temp_store = MEMORY');
      db.pragma('cache_size = -64000');
      break;
    case 'realistic_fast':
      db.pragma('journal_mode = WAL');
      db.pragma('synchronous = NORMAL');
      db.pragma('temp_store = MEMORY');
      db.pragma('cache_size = -64000');
      break;
    case 'realistic':
      db.pragma('journal_mode = WAL');
      db.pragma('synchronous = FULL');
      break;
    case 'default':
      // leave engine defaults
      /*
      [sqlite] defaults before pragmas: {
        journal_mode: 'wal',
        synchronous: 1,
        temp_store: 0,
        locking_mode: 'normal',
        cache_size: -16000,
        foreign_keys: 1
      }
      */
      break;
    case 'baseline':
      db.pragma('journal_mode = DELETE');
      db.pragma('synchronous = FULL');
      break;
  }
}

export function sqliteModeRank(mode: SqliteMode): number {
  switch (mode) {
    case 'baseline':
      return 0;
    case 'default':
      return 1;
    case 'realistic':
      return 2;
    case 'realistic_fast':
      return 3;
    case 'realistic_cached':
      return 4;
    case 'fastest':
      return 5;
  }
}

export async function ensureSqliteDirExists(
  file: string,
  mode: SqliteMode,
): Promise<void> {
  if (mode === 'fastest') return;
  const dir = path.dirname(file);
  await fs.promises.mkdir(dir, { recursive: true });
}

export function ensureSqliteDirExistsSync(
  file: string,
  mode: SqliteMode,
): void {
  if (mode === 'fastest') return;
  const dir = path.dirname(file);
  fs.mkdirSync(dir, { recursive: true });
}
