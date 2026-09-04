import { execFileSync } from 'node:child_process';
import { describesMissingStockInterface, stockInterfaceError } from '../stock-interface.js';


import { assertLeasedContainer } from '../backend-reset-guard.js';
import type { LeasedDatabase } from '../backend-reset-guard.js';
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

const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;

const stockUpdateSql = (itemName: string, warehouseName: string, quantity: number): string => `
WITH foreign_keys AS (
  SELECT source_ns.nspname AS source_schema, source.relname AS source_table,
         source_column.attname AS source_column,
         target_ns.nspname AS target_schema, target.relname AS target_table,
         target_column.attname AS target_column
  FROM pg_constraint constraint_record
  JOIN pg_class source ON source.oid = constraint_record.conrelid
  JOIN pg_namespace source_ns ON source_ns.oid = source.relnamespace
  JOIN pg_attribute source_column ON source_column.attrelid = source.oid
    AND source_column.attnum = constraint_record.conkey[1]
  JOIN pg_class target ON target.oid = constraint_record.confrelid
  JOIN pg_namespace target_ns ON target_ns.oid = target.relnamespace
  JOIN pg_attribute target_column ON target_column.attrelid = target.oid
    AND target_column.attnum = constraint_record.confkey[1]
  WHERE constraint_record.contype = 'f'
    AND cardinality(constraint_record.conkey) = 1
    AND cardinality(constraint_record.confkey) = 1
), named_tables AS (
  SELECT table_schema, table_name
  FROM information_schema.columns
  WHERE table_schema = 'public' AND column_name = 'name'
), quantity_columns AS (
  SELECT table_schema, table_name, column_name
  FROM information_schema.columns
  WHERE table_schema = 'public'
    AND column_name IN ('quantity', 'qty', 'stock', 'on_hand', 'available')
)
SELECT format(
  'WITH target AS (SELECT item.%1$I AS item_id, warehouse.%2$I AS warehouse_id '
  'FROM public.%3$I item, public.%4$I warehouse '
  'WHERE item.name = %5$L AND warehouse.name = %6$L) '
  'UPDATE public.%7$I stock SET %8$I = %9$s FROM target '
  'WHERE stock.%10$I = target.item_id AND stock.%11$I = target.warehouse_id;',
  item.target_column, warehouse.target_column, item.target_table, warehouse.target_table,
  ${sqlString(itemName)}, ${sqlString(warehouseName)}, quantity.table_name, quantity.column_name,
  ${quantity}, item.source_column, warehouse.source_column)
FROM quantity_columns quantity
JOIN foreign_keys item ON item.source_schema = quantity.table_schema
  AND item.source_table = quantity.table_name
JOIN named_tables item_name ON item_name.table_schema = item.target_schema
  AND item_name.table_name = item.target_table
JOIN foreign_keys warehouse ON warehouse.source_schema = quantity.table_schema
  AND warehouse.source_table = quantity.table_name
  AND warehouse.source_column <> item.source_column
JOIN named_tables warehouse_name ON warehouse_name.table_schema = warehouse.target_schema
  AND warehouse_name.table_name = warehouse.target_table
ORDER BY quantity.table_name, item.source_column, warehouse.source_column
\\gexec
`;

export function resetPostgres({ lease, exec = execFileSync }:
  { lease: LeasedDatabase; exec?: TextCommandExecutor }): string {
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  exec('docker', ['exec', containerId, 'psql', '-U', POSTGRES_USER,
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
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database provenance');
  const sql = "SELECT format('SELECT %L WHERE EXISTS (SELECT 1 FROM %I.%I WHERE %I::text = %L LIMIT 1);', "
    + "table_schema || '.' || table_name || '.' || column_name, table_schema, table_name, "
    + `column_name, ${sqlString(marker)}) FROM information_schema.columns `
    + "WHERE table_schema = 'public' ORDER BY table_name, ordinal_position\n\\gexec\n";
  const output = exec('docker', ['exec', '-i', containerId,
    'psql', '-U', POSTGRES_USER, '-d', lease.resources.database,
    '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  const matches = output ? output.split(/\r?\n/).filter(Boolean) : [];
  return { ok: matches.length > 0, verified: true, matches: matches.length,
    reason: matches.length
      ? 'the application marker exists in the leased PostgreSQL database'
      : 'the application marker is absent from the leased PostgreSQL database' };
}

export function setPostgresStock({ item, warehouse, quantity, lease, exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; lease: LeasedDatabase;
  exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS,
    'direct database write');
  const dbName = lease.resources.database;
  let output: string;
  try {
    output = exec('docker', ['exec', '-i', container,
      'psql', '-U', POSTGRES_USER, '-d', dbName, '-v', 'ON_ERROR_STOP=1', '-At'],
    { encoding: 'utf8', input: stockUpdateSql(item, warehouse, quantity),
      stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    // psql refusing the statement over an absent table or column is the
    // application not providing the interface, not a harness fault.
    const detail = streams(error, 'stdout', 'stderr', 'message');
    if (describesMissingStockInterface(detail)) {
      throw stockInterfaceError(detail.trim().slice(-300), { cause: error });
    }
    throw error;
  }
  if ((output.match(/UPDATE 1\b/g) ?? []).length === 1) {
    return { backend: 'postgres', item, warehouse, quantity };
  }
  throw stockInterfaceError(`could not locate one relational stock row for ${item} / ${warehouse}`);
}

export function preparePostgresDatabase({ lease, name, expectedName, wipe,
  exec = execFileSync }: {
  lease: LeasedDatabase; name: string; expectedName: string; wipe: boolean;
  exec?: TextCommandExecutor;
}): string {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  const container = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database mutation');
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
