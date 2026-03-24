import convex from './convex.ts';
import { spacetimedb } from './spacetimedb.ts';
import bun from './bun.ts';
import postgres_rpc from './rpc/postgres_rpc.ts';
import cockroach_rpc from './rpc/cockroach_rpc.ts';
import sqlite_rpc from './rpc/sqlite_rpc.ts';
import supabase_rpc from './rpc/supabase_rpc.ts';
import planetscale_pg_rpc from './rpc/planetscale_pg_rpc.ts';
import type { ReducerConnector, RpcConnector } from '../core/connectors.ts';
import type { ConnectorKey, ConnectorRuntimeConfig } from '../config.ts';

export type { ConnectorKey } from '../config.ts';

export type ConnectorFactory = (
  config: ConnectorRuntimeConfig,
) => ReducerConnector | RpcConnector;

export const CONNECTORS = {
  convex: (config) => convex(config.convexUrl),
  spacetimedb: (config) => spacetimedb(config),
  spacetimedbRustClient: spacetimedb,
  bun: (config) => bun(config.bunUrl),
  postgres_rpc: () => postgres_rpc(),
  cockroach_rpc: () => cockroach_rpc(),
  sqlite_rpc: () => sqlite_rpc(),
  supabase_rpc: () => supabase_rpc(),
  planetscale_pg_rpc: () => planetscale_pg_rpc(),
} satisfies Record<ConnectorKey, ConnectorFactory>;
