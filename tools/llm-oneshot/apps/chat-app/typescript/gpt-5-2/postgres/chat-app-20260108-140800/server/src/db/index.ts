import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';

const DATABASE_URL =
  process.env.DATABASE_URL ||
  'postgres://postgres:postgres@localhost:5432/chat-app';

export const sql = postgres(DATABASE_URL, {
  max: 10,
  idle_timeout: 20,
});

export const db = drizzle(sql);
