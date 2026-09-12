import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { DATABASE_IMAGES } from '../src/stacks/database-containers.js';

import type { TextCommandOptions } from '../src/runtime/command-executor.js';
import { proveMongoDbUse } from '../src/stacks/backends/mongodb-operations.js';
import { provePostgresUse, resetPostgres } from '../src/stacks/backends/postgres-operations.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../src/stacks/hosted-database-identity.js';

const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;
const docker = (args: readonly string[], options: Partial<TextCommandOptions> = {}): string =>
  execFileSync('docker', args, { encoding: 'utf8', stdio: 'pipe', timeout: 120_000, ...options });

test('runtime database provenance works against the Docker services', () => {
  const suffix = `${process.pid}_${Date.now()}`;
  const marker = `application-proof-${suffix}`;
  const postgresDatabase = `application_proof_${suffix}`;
  const mongoDatabase = `application_proof_${suffix}`;
  const container = (name: string): { name: string; id: string } => ({ name,
    id: docker(['inspect', '--format', '{{.Id}}', name]).trim() });
  try {
    docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
      '-v', 'ON_ERROR_STOP=1', '-c', `CREATE DATABASE ${postgresDatabase} OWNER ${POSTGRES_USER};`]);
    docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', postgresDatabase,
      '-v', 'ON_ERROR_STOP=1', '-c',
      `CREATE TABLE account (username text NOT NULL); INSERT INTO account VALUES ('${marker}');`]);
    docker(['exec', 'stack-bench-mongodb', 'mongosh', mongoDatabase, '--quiet', '--eval',
      `db.account.insertOne({ username: ${JSON.stringify(marker)} })`]);

    assert.equal(provePostgresUse({ lease: { resources: { database: postgresDatabase,
      container: container('stack-bench-postgres') } }, marker }).ok, true);
    assert.equal(proveMongoDbUse({ lease: { resources: { database: mongoDatabase,
      container: container('stack-bench-mongodb') } }, marker }).ok, true);
  } finally {
    try {
      docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
        '-v', 'ON_ERROR_STOP=1', '-c', `DROP DATABASE IF EXISTS ${postgresDatabase} WITH (FORCE);`]);
    } catch { /* preserve the test failure */ }
    try {
      docker(['exec', 'stack-bench-mongodb', 'mongosh', mongoDatabase, '--quiet', '--eval',
        'db.dropDatabase()']);
    } catch { /* preserve the test failure */ }
  }
});

test('PostgreSQL reset removes migrated structures and preserves neighboring databases', { timeout: 60_000 }, async () => {
  const container = 'stack-bench-reset-regression-' + process.pid;
  const id = docker(['run', '--pull=never', '--rm', '-d', '--name', container,
    '--network', 'none', '--memory', '384m', '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', DATABASE_IMAGES.postgres]).trim();
  const database = (name: string, sql: string, user = 'postgres'): string =>
    docker(['exec', '-i', id, 'psql', '-U', user, '-d', name, '-v', 'ON_ERROR_STOP=1', '-At'], { input: sql }).trim();
  try {
    for (let attempt = 0; ; attempt++) {
      try { database('postgres', 'SELECT 1'); break; }
      catch (error) { if (attempt >= 60) throw error; await delay(250); }
    }
    database('postgres', 'CREATE USER appuser; CREATE DATABASE reset_target OWNER appuser; CREATE DATABASE neighbor;');
    database('neighbor', 'CREATE TABLE evidence(value integer); INSERT INTO evidence VALUES(42);');
    const lease = { resources: { database: 'reset_target', container: { name: container, id },
      network: { name: 'owned', id, namespaceContainerId: id, hostAddresses: [], services: [],
        firewallSha256: null, firewallInstalledAt: null } } };
    const initialize = () => database('reset_target', `
CREATE TABLE warehouses(name text PRIMARY KEY);
CREATE TABLE stock(warehouse text REFERENCES warehouses, quantity integer);
CREATE TABLE migrations(name text PRIMARY KEY);
INSERT INTO warehouses VALUES('East'); INSERT INTO stock VALUES('East', 7);
INSERT INTO migrations VALUES('initial');
CREATE TABLE warehouse(id text PRIMARY KEY);
INSERT INTO warehouse SELECT name FROM warehouses;
ALTER TABLE stock ADD COLUMN warehouse_id text REFERENCES warehouse;
CREATE FUNCTION sync_stock() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.warehouse_id=NEW.warehouse; RETURN NEW; END $$;
CREATE TRIGGER sync_stock BEFORE INSERT ON stock FOR EACH ROW EXECUTE FUNCTION sync_stock();
CREATE SCHEMA migration_meta; CREATE TABLE migration_meta.version(n integer); INSERT INTO migration_meta.version VALUES(2);
`, 'appuser');
    initialize();
    // The former reset erased the history but left the later trigger active.
    database('reset_target', 'TRUNCATE migrations, stock, warehouse, warehouses CASCADE;');
    assert.throws(() => database('reset_target', "INSERT INTO warehouses VALUES('East'); INSERT INTO stock(warehouse,quantity) VALUES('East',7);"), /foreign key/);
    resetPostgres({ lease });
    assert.equal(database('reset_target', "SELECT to_regclass('public.stock') IS NULL AND to_regnamespace('migration_meta') IS NULL;"), 't');
    initialize();
    assert.equal(database('reset_target', 'SELECT quantity FROM stock;'), '7');
    assert.equal(database('neighbor', 'SELECT value FROM evidence;'), '42');
    assert.equal(database('postgres', "SELECT rolcreatedb OR rolsuper FROM pg_roles WHERE rolname='appuser';"), 'f');
  } finally { docker(['rm', '-f', id]); }
});
