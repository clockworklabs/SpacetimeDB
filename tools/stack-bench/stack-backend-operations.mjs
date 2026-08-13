import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { leasedSpacetimeTarget } from './spacetime-target.mjs';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;

function inspectContainer(container, exec, purpose) {
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing ${purpose}`);
  }
}

export function resetPostgres({ lease, exec = execFileSync }) {
  inspectContainer(lease.resources.container, exec, 'reset');
  exec('docker', ['exec', lease.resources.container.name, 'psql', '-U', 'stackbench',
    '-d', lease.resources.database, '-c',
    `DO $$ DECLARE r RECORD; BEGIN
       FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
         EXECUTE 'TRUNCATE TABLE ' || quote_ident(r.tablename) || ' RESTART IDENTITY CASCADE';
       END LOOP;
     END $$;`], { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset postgres database ${lease.resources.database}`;
}

export function resetMongoDb({ lease, exec = execFileSync }) {
  inspectContainer(lease.resources.container, exec, 'reset');
  exec('docker', ['exec', lease.resources.container.name, 'mongosh', lease.resources.database,
    '--quiet', '--eval', 'db.dropDatabase()'], { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset mongodb database ${lease.resources.database}`;
}

export function resetSpacetime({ lease, app, exec = execFileSync }) {
  const modulePath = resolve(app, 'backend', 'spacetimedb');
  if (!existsSync(modulePath)) throw new Error(`module directory is missing: ${modulePath}`);
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  const containerModule = '/app/backend/spacetimedb';
  try {
    exec('docker', ['exec', container.name, 'test', '-d', containerModule],
      { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    exec('docker', ['exec', '-w', containerModule, container.name, '/deps/spacetimedb-cli',
      'publish', lease.resources.module, '--module-path', containerModule,
      '-s', target.containerUri, '--delete-data', '-y'],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const detail = `${error.stdout ?? ''}${error.stderr ?? ''}`.trim() || error.message;
    throw new Error(`SpacetimeDB reset publish failed in leased container: ${detail}`, { cause: error });
  }
  return `reset spacetime module ${lease.resources.module} on ${lease.resources.serverUri}`;
}

const sqlString = value => `'${String(value).replaceAll("'", "''")}'`;

export function setPostgresStock({ item, warehouse, quantity, dbName, exec = execFileSync,
  containers = {} }) {
  const container = containers.postgres ?? process.env.POSTGRES_CONTAINER ?? 'stack-bench-postgres';
  const sql = `UPDATE stock SET quantity = ${quantity} WHERE item_id = `
    + `(SELECT id FROM item WHERE name = ${sqlString(item)}) AND warehouse_id = `
    + `(SELECT id FROM warehouse WHERE name = ${sqlString(warehouse)})`;
  const output = exec('docker', ['exec', container,
    'psql', '-U', 'stackbench', '-d', dbName, '-c', sql],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  if (!/UPDATE 1\b/.test(output)) {
    throw new Error(`stock row for ${item} / ${warehouse} was not updated (${output.trim().slice(-120)})`);
  }
  return { backend: 'postgres', item, warehouse, quantity };
}

export function setMongoDbStock({ item, warehouse, quantity, dbName, exec = execFileSync,
  containers = {} }) {
  const container = containers.mongodb ?? process.env.MONGO_CONTAINER ?? 'stack-bench-mongodb';
  const script = `
    const it = db.item.findOne({ name: ${JSON.stringify(item)} });
    const wh = db.warehouse.findOne({ name: ${JSON.stringify(warehouse)} });
    if (!it || !wh) { print('MISSING'); quit(1); }
    const iid = it._id ?? it.id, wid = wh._id ?? wh.id;
    const r = db.stock.updateOne(
      { $or: [ { item_id: iid, warehouse_id: wid }, { itemId: iid, warehouseId: wid } ] },
      { $set: { quantity: ${quantity} } });
    print(r.matchedCount === 1 ? 'OK' : 'NOMATCH');
  `;
  const output = exec('docker', ['exec', container,
    'mongosh', dbName, '--quiet', '--eval', script],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  if (!/OK/.test(output)) {
    throw new Error(`could not find ${item} / ${warehouse} in the required collections `
      + `(${output.trim().slice(0, 80)})`);
  }
  return { backend: 'mongodb', item, warehouse, quantity };
}

export function setSpacetimeStock({ item, warehouse, quantity, spacetime, exec = execFileSync }) {
  if (!spacetime?.buildContainer) {
    throw new Error('SpacetimeDB build container is unavailable for direct SQL');
  }
  const query = sql => exec('docker', ['exec', spacetime.buildContainer.name,
    '/deps/spacetimedb-cli', 'sql', spacetime.mod, '-s', spacetime.containerUri, sql],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  const idOf = (table, name) => {
    const output = query(`select id from ${table} where name = ${sqlString(name)}`);
    const match = output.match(/^\s*(\d+)\s*$/m);
    if (!match) throw new Error(`no ${table} named "${name}" — is the schema as the spec requires?`);
    return match[1];
  };
  const itemId = idOf('item', item);
  const warehouseId = idOf('warehouse', warehouse);
  query(`update stock set quantity = ${quantity} where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  const verified = query(`select quantity from stock where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  if (!new RegExp(`^\\s*${quantity}\\s*$`, 'm').test(verified)) {
    throw new Error(`stock row for ${item} / ${warehouse} did not update to ${quantity}`);
  }
  return { backend: 'spacetime', item, warehouse, quantity };
}

export function preparePostgresDatabase({ lease, name, expectedName, wipe, exec = execFileSync }) {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  const container = lease.resources.container.name;
  inspectContainer(lease.resources.container, exec, 'database mutation');
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

export function prepareMongoDbDatabase({ lease, name, expectedName, wipe, exec = execFileSync }) {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  inspectContainer(lease.resources.container, exec, 'database mutation');
  if (wipe) {
    try {
      exec('docker', ['exec', lease.resources.container.name, 'mongosh', name, '--quiet',
        '--eval', 'db.dropDatabase()'], { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
      console.error(`  wiped ${name} — a build starts on an empty database`);
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${String(error.message).split('\n')[0]}`, { cause: error });
    }
  }
  return name;
}

export function prepareSpacetimeDatabase({ lease, name, wipe, exec = execFileSync,
  cli, expectedServerUri, expectedModule }) {
  if (lease.resources.serverUri !== expectedServerUri
    || lease.resources.module !== expectedModule) {
    throw new Error('SpacetimeDB lease target does not match the harness run');
  }
  if (!wipe) return name;
  try {
    exec(cli, ['delete', lease.resources.module, '-s', lease.resources.serverUri, '-y'],
      { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    console.error(`  deleted module ${lease.resources.module} — a build starts with none published`);
  } catch (error) {
    const detail = `${error.stdout ?? ''}${error.stderr ?? ''}${error.message ?? ''}`;
    // `spacetime delete` has used each of these messages for an absent database
    // across CLI/API versions. A clean first run has nothing to delete, so only
    // that specific condition is idempotent; auth, transport, and server errors
    // must still stop the run.
    if (!/(?:404\s+Not Found|no such database|failed to find database)/i.test(detail)) {
      throw new Error(`could not delete prior module ${lease.resources.module}: ${detail.trim().split('\n')[0]}`,
        { cause: error });
    }
  }
  return name;
}

export function prepareResourceFreeDatabase({ name }) {
  return name;
}
