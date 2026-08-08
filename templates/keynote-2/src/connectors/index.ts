import convex from './convex.ts';
import { spacetimedb } from './spacetimedb.ts';
import bun from './bun.ts';
import postgres_rpc from './rpc/postgres_rpc.ts';
import cockroach_rpc from './rpc/cockroach_rpc.ts';
import sqlite_rpc from './rpc/sqlite_rpc.ts';
import supabase_rpc from './rpc/supabase_rpc.ts';
import planetscale_pg_rpc from './rpc/planetscale_pg_rpc.ts';
import postgres_storedproc_rpc from './rpc/postgres_storedproc_rpc.ts';
import postgres_direct from './postgres_direct.ts';
import type { ReducerConnector, RpcConnector } from '../core/connectors.ts';
import type {
  ConnectorKey,
  ConnectorRuntimeConfig,
  SpacetimeConnectorConfig,
} from '../config.ts';

export type { ConnectorKey } from '../config.ts';

export type ConnectorFactory = (
  config: ConnectorRuntimeConfig,
) => ReducerConnector | RpcConnector;

const toSpacetimeConfig = (
  config: ConnectorRuntimeConfig,
): SpacetimeConnectorConfig => ({
  initialBalance: config.initialBalance,
  stdbCompression: config.stdbCompression,
  stdbConfirmedReads: config.stdbConfirmedReads,
  stdbModule: config.stdbModule,
  stdbUrl: config.stdbUrl,
});

export const CONNECTORS = {
  convex: (config) => convex(config.convexUrl),
  spacetimedb: (config) => spacetimedb(toSpacetimeConfig(config)),
  bun: (config) => bun(config.bunUrl),
  postgres_rpc: () => postgres_rpc(),
  cockroach_rpc: () => cockroach_rpc(),
  sqlite_rpc: () => sqlite_rpc(),
  supabase_rpc: () => supabase_rpc(),
  planetscale_pg_rpc: () => planetscale_pg_rpc(),
  postgres_storedproc_rpc: () => postgres_storedproc_rpc(),
  postgres_direct: (config) => postgres_direct({
    url: process.env.PG_URL ?? 'postgres://postgres:postgres@127.0.0.1:5432/postgres',
    poolMax: config.poolMax,
    initialBalance: BigInt(config.initialBalance),
  }),
} satisfies Record<ConnectorKey, ConnectorFactory>;
