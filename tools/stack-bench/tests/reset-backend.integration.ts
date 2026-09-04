import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import test from 'node:test';

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

test('PostgreSQL reset preserves schema and does not touch another lease database', () => {
  const suffix = `${process.pid}_${Date.now()}`;
  const target = `application_reset_${suffix}`;
  const neighbor = `application_neighbor_${suffix}`;
  const container = 'stack-bench-postgres';
  const database = (name: string, sql: string): string =>
    docker(['exec', container, 'psql', '-U', POSTGRES_USER,
      '-d', name, '-v', 'ON_ERROR_STOP=1', '-tAc', sql]);
  try {
    const id = docker(['inspect', '--format', '{{.Id}}', container]).trim();
    for (const name of [target, neighbor]) {
      docker(['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
        '-v', 'ON_ERROR_STOP=1', '-c', `CREATE DATABASE ${name} OWNER ${POSTGRES_USER};`]);
      database(name, 'CREATE TABLE item (id bigserial PRIMARY KEY, name text NOT NULL); '
        + "INSERT INTO item(name) VALUES ('kept schema');");
    }
    resetPostgres({ lease: { resources: { database: target,
      container: { name: container, id } } } });
    assert.equal(database(target,
      "SELECT to_regclass('public.item') IS NOT NULL, count(*) FROM item;").trim(), 't|0');
    database(target, "INSERT INTO item(name) VALUES ('reseeded');");
    assert.equal(database(target, "SELECT id FROM item WHERE name = 'reseeded';").trim(), '1');
    assert.equal(database(neighbor, 'SELECT count(*) FROM item;').trim(), '1');
  } finally {
    for (const name of [target, neighbor]) {
      try {
        docker(['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
          '-v', 'ON_ERROR_STOP=1', '-c', `DROP DATABASE IF EXISTS ${name} WITH (FORCE);`]);
      } catch { /* preserve the test failure */ }
    }
  }
});
