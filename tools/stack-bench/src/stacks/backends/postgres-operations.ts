import { execFileSync } from 'node:child_process';


import { assertLeasedContainer } from '../backend-reset-guard.js';
import { databaseContainerName } from '../database-containers.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../hosted-database-identity.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

type LeasedDatabase = {
  resources: { container: { id: string; name: string }; database: string };
};
const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;

export function resetPostgres({ lease, exec = execFileSync }:
  { lease: LeasedDatabase; exec?: TextCommandExecutor }): string {
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  exec('docker', ['exec', lease.resources.container.name, 'psql', '-U', POSTGRES_USER,
    '-d', lease.resources.database, '-v', 'ON_ERROR_STOP=1', '-c',
    "DO $block$ DECLARE tables text; BEGIN "
      + "SELECT string_agg(format('%I.%I', schemaname, tablename), ', ') INTO tables "
      + "FROM pg_tables WHERE schemaname = 'public'; "
      + "IF tables IS NOT NULL THEN EXECUTE 'TRUNCATE TABLE ' || tables "
      + "|| ' RESTART IDENTITY CASCADE'; END IF; END $block$;"],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset postgres database ${lease.resources.database}`;
}

export function provePostgresUse({ lease, marker, exec = execFileSync }:
  { lease: LeasedDatabase; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error('PostgreSQL provenance requires a non-empty application marker');
  }
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database provenance');
  const sql = "SELECT format('SELECT %L WHERE EXISTS (SELECT 1 FROM %I.%I WHERE %I::text = %L LIMIT 1);', "
    + "table_schema || '.' || table_name || '.' || column_name, table_schema, table_name, "
    + `column_name, ${sqlString(marker)}) FROM information_schema.columns `
    + "WHERE table_schema = 'public' ORDER BY table_name, ordinal_position\n\\gexec\n";
  const output = exec('docker', ['exec', '-i', lease.resources.container.name,
    'psql', '-U', POSTGRES_USER, '-d', lease.resources.database,
    '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  const matches = output ? output.split(/\r?\n/).filter(Boolean) : [];
  return { ok: matches.length > 0, verified: true, matches: matches.length,
    reason: matches.length
      ? 'the application marker exists in the leased PostgreSQL database'
      : 'the application marker is absent from the leased PostgreSQL database' };
}

export function setPostgresStock({ item, warehouse, quantity, dbName, exec = execFileSync,
  containers = {} }: {
  item: string; warehouse: string; quantity: number; dbName: string;
  exec?: TextCommandExecutor; containers?: { postgres?: string };
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = containers.postgres ?? databaseContainerName('postgres');
  const sql = `UPDATE stock SET quantity = ${quantity} WHERE item_id = `
    + `(SELECT id FROM item WHERE name = ${sqlString(item)}) AND warehouse_id = `
    + `(SELECT id FROM warehouse WHERE name = ${sqlString(warehouse)})`;
  let output: string;
  try {
    output = exec('docker', ['exec', container,
      'psql', '-U', POSTGRES_USER, '-d', dbName, '-c', sql],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    const detail = streams(error, 'stderr', 'stdout').trim().slice(-240);
    throw new Error('direct stock correction requires singular tables '
      + '`item(id, name)`, `warehouse(id, name)`, and '
      + `\`stock(item_id, warehouse_id, quantity)\`: ${detail || streams(error, 'message')}`,
    { cause: error });
  }
  if (!/UPDATE 1\b/.test(output)) {
    throw new Error(`stock row for ${item} / ${warehouse} was not updated (${output.trim().slice(-120)})`);
  }
  return { backend: 'postgres', item, warehouse, quantity };
}

export function preparePostgresDatabase({ lease, name, expectedName, wipe,
  exec = execFileSync }: {
  lease: LeasedDatabase; name: string; expectedName: string; wipe: boolean;
  exec?: TextCommandExecutor;
}): string {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  const container = lease.resources.container.name;
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'database mutation');
  try {
    exec('docker', ['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
      '-c', `CREATE DATABASE ${name} OWNER ${POSTGRES_USER};`],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const exists = exec('docker', ['exec', container, 'psql', '-U', POSTGRES_USER,
      '-d', 'postgres', '-tAc', `SELECT 1 FROM pg_database WHERE datname = '${name}';`],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
    if (exists !== '1') throw error;
  }
  if (wipe) {
    try {
      exec('docker', ['exec', container, 'psql', '-U', POSTGRES_USER, '-d', name,
        '-c', 'DROP SCHEMA public CASCADE; CREATE SCHEMA public; '
            + `GRANT ALL ON SCHEMA public TO ${POSTGRES_USER};`],
      { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
      console.error(`  wiped ${name} (schema dropped) — a build starts on an empty database`);
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${streams(error, 'message').split('\n')[0]}`,
      { cause: error });
    }
  }
  return name;
}
