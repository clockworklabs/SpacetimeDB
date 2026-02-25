import convex from './convex.ts';
import { spacetimedb } from './spacetimedb.ts';
import bun from './bun.ts';
import { postgres } from './postgres.ts';
import { postgres_no_rpc } from './postgres.ts';
import postgres_rpc from './rpc/postgres_rpc.ts';
import cockroach_rpc from './rpc/cockroach_rpc.ts';
import sqlite_rpc from './rpc/sqlite_rpc.ts';
import supabase_rpc from './rpc/supabase_rpc.ts';
import planetscale_pg_rpc from './rpc/planetscale_pg_rpc.ts';

export const CONNECTORS = {
  convex,
  spacetimedb,
  bun,
  postgres,
  postgres_no_rpc,
  postgres_rpc,
  cockroach_rpc,
  sqlite_rpc,
  supabase_rpc,
  planetscale_pg_rpc,
};
export type ConnectorKey = keyof typeof CONNECTORS;
