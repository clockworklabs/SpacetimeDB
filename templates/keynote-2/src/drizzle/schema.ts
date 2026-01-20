import { pgTable, integer, bigint as pgBigint } from 'drizzle-orm/pg-core';
import { sqliteTable, integer as sqliteInt } from 'drizzle-orm/sqlite-core';

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
