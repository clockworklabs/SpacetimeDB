import postgres from './direct/postgres.ts';
import cockroach from './direct/cockroach.ts';
import sqlite from './direct/sqlite.ts';
import supabase from './supabase.ts';
import convex from './convex.ts';
import { spacetimedb } from './spacetimedb.ts';
import bun from './bun.ts';
import postgres_drizzle from './drizzle/postgres_drizzle.ts';
import { cockroach_drizzle } from './drizzle/cockroach_drizzle.ts';
import sqlite_drizzle from './drizzle/sqlite_drizzle.ts';
import postgres_rpc from './rpc/postgres_rpc.ts';
import cockroach_rpc from './rpc/cockroach_rpc.ts';
import sqlite_rpc from './rpc/sqlite_rpc.ts';
import supabase_rpc from './rpc/supabase_rpc.ts';
import planetscale_pg_rpc from './rpc/planetscale_pg_rpc.ts';

export const CONNECTORS = {
  postgres,
  cockroach,
  sqlite,
  supabase,
  convex,
  spacetimedb,
  bun,
  postgres_drizzle,
  cockroach_drizzle,
  sqlite_drizzle,
  postgres_rpc,
  cockroach_rpc,
  sqlite_rpc,
  supabase_rpc,
  planetscale_pg_rpc,
};
export type ConnectorKey = keyof typeof CONNECTORS;
