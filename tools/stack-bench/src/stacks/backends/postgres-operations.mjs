import { execFileSync } from 'node:child_process';

import { assertLeasedContainer } from '../backend-reset-guard.mjs';
import { databaseContainerName } from '../database-containers.mjs';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = value => `'${String(value).replaceAll("'", "''")}'`;

export function resetPostgres({ lease, exec = execFileSync }) {
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  exec('docker', ['exec', lease.resources.container.name, 'psql', '-U', 'stackbench',
    '-d', lease.resources.database, '-v', 'ON_ERROR_STOP=1', '-c',
    "DO $stackbench$ DECLARE tables text; BEGIN "
      + "SELECT string_agg(format('%I.%I', schemaname, tablename), ', ') INTO tables "
      + "FROM pg_tables WHERE schemaname = 'public'; "
      + "IF tables IS NOT NULL THEN EXECUTE 'TRUNCATE TABLE ' || tables "
      + "|| ' RESTART IDENTITY CASCADE'; END IF; END $stackbench$;"],
  { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset postgres database ${lease.resources.database}`;
}

export function provePostgresUse({ lease, exec = execFileSync }) {
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database provenance');
  const sql = "SELECT format('SELECT %L WHERE EXISTS (SELECT 1 FROM %I.%I LIMIT 1);', "
    + "schemaname || '.' || tablename, schemaname, tablename) "
    + "FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename\n\\gexec\n";
  const output = exec('docker', ['exec', '-i', lease.resources.container.name,
    'psql', '-U', 'stackbench', '-d', lease.resources.database,
    '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  const populatedTables = output ? output.split(/\r?\n/).filter(Boolean) : [];
  return { ok: populatedTables.length > 0, verified: true,
    populatedTables: populatedTables.length,
    reason: populatedTables.length
      ? `${populatedTables.length} leased PostgreSQL table(s) contain startup data`
      : 'the leased PostgreSQL database remained empty after application startup' };
}

export function setPostgresStock({ item, warehouse, quantity, dbName, exec = execFileSync,
  containers = {} }) {
  const container = containers.postgres ?? databaseContainerName('postgres');
  const sql = `UPDATE stock SET quantity = ${quantity} WHERE item_id = `
    + `(SELECT id FROM item WHERE name = ${sqlString(item)}) AND warehouse_id = `
    + `(SELECT id FROM warehouse WHERE name = ${sqlString(warehouse)})`;
  let output;
  try {
    output = exec('docker', ['exec', container,
      'psql', '-U', 'stackbench', '-d', dbName, '-c', sql],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    const detail = `${error.stderr ?? ''}${error.stdout ?? ''}`.trim().slice(-240);
    throw new Error('direct stock correction requires singular tables '
      + '`item(id, name)`, `warehouse(id, name)`, and '
      + `\`stock(item_id, warehouse_id, quantity)\`: ${detail || error.message}`, { cause: error });
  }
  if (!/UPDATE 1\b/.test(output)) {
    throw new Error(`stock row for ${item} / ${warehouse} was not updated (${output.trim().slice(-120)})`);
  }
  return { backend: 'postgres', item, warehouse, quantity };
}

export function preparePostgresDatabase({ lease, name, expectedName, wipe, exec = execFileSync }) {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  const container = lease.resources.container.name;
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'database mutation');
  try {
    exec('docker', ['exec', container, 'psql', '-U', 'stackbench', '-d', 'postgres',
      '-c', `CREATE DATABASE ${name} OWNER stackbench;`],
    { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const exists = exec('docker', ['exec', container, 'psql', '-U', 'stackbench',
      '-d', 'postgres', '-tAc', `SELECT 1 FROM pg_database WHERE datname = '${name}';`],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
    if (exists !== '1') throw error;
  }
  if (wipe) {
    try {
      exec('docker', ['exec', container, 'psql', '-U', 'stackbench', '-d', name,
        '-c', 'DROP SCHEMA public CASCADE; CREATE SCHEMA public; '
            + 'GRANT ALL ON SCHEMA public TO stackbench;'],
      { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
      console.error(`  wiped ${name} (schema dropped) — a build starts on an empty database`);
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${String(error.message).split('\n')[0]}`, { cause: error });
    }
  }
  return name;
}
