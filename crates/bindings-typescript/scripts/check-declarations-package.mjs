import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { build } from 'tsup';

for (const load of [
  specifier => import(specifier),
  createRequire(import.meta.url),
]) {
  const { schema, table, t } = await load('spacetimedb/schema');
  const { createClient } = await load('spacetimedb');
  assert.equal((await load('spacetimedb/sdk')).createClient, createClient);
  const s = schema({
    person: table({ public: true }, t.row({ name: t.string() })),
  });
  const client = createClient({
    default: s,
    add: s.reducer({ name: t.string() }),
  });
  assert.equal(client.tables.person.toSql(), 'SELECT * FROM "person"');
  assert.equal(client.reducers.add.name, 'add');
  assert.equal(typeof client.DbConnection.builder, 'function');
}

const outDir = await mkdtemp(join(tmpdir(), 'spacetimedb-consumer-'));
try {
  await build({
    entry: ['tests/declarations-consumer/index.ts'],
    platform: 'browser',
    format: 'esm',
    outDir,
    config: false,
    tsconfig: 'tests/declarations-consumer/tsconfig.json',
    noExternal: ['spacetimedb'],
    silent: true,
  });
} finally {
  await rm(outDir, { recursive: true, force: true });
}
console.log('Declaration package consumers passed (ESM, CommonJS, browser).');
