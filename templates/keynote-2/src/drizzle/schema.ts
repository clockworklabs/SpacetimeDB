import { pgTable, integer, bigint as pgBigint } from 'drizzle-orm/pg-core';
import { sqliteTable, integer as sqliteInt } from 'drizzle-orm/sqlite-core';

// GHSA-gpj5-g38j-94v9: Drizzle fails to escape quotes in SQL identifiers,
// allowing SQL injection if a request controls a table/column/alias name.
// These queries use the fixed schema names below; request values are parameters,
// never identifiers passed to sql.identifier(), .as(), or table constructors.
export const pgAccounts = pgTable('accounts', {
  id: integer('id').primaryKey(),
  balance: pgBigint('balance', { mode: 'bigint' }).notNull(),
});

export const crdbAccounts = pgTable('accounts', {
  id: integer('id').primaryKey(),
  balance: pgBigint('balance', { mode: 'bigint' }).notNull(),
});

export const sqliteAccounts = sqliteTable('accounts', {
  id: sqliteInt('id').primaryKey(),
  balance: sqliteInt('balance').notNull(),
});
