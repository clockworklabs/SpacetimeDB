import { execFileSync } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { build } from 'tsup';
import { test } from 'vitest';

// Use native Node loaders so these checks exercise package exports, not Vite's resolver.
test('built module schemas work with ESM and CommonJS clients', () => {
  execFileSync(
    process.execPath,
    [
      '--input-type=module',
      '--eval',
      `
    import assert from 'node:assert/strict';
    import { createRequire } from 'node:module';

    for (const load of [specifier => import(specifier), createRequire(import.meta.url)]) {
      const { schema, table, t } = await load('spacetimedb/schema');
      const { createClient } = await load('spacetimedb');
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
  `,
    ],
    { stdio: 'pipe' }
  );
});

test('module schema clients bundle for browsers', async () => {
  const outDir = await mkdtemp(join(tmpdir(), 'spacetimedb-consumer-'));
  try {
    await build({
      entry: ['tests/consumer/module-schema-browser.ts'],
      platform: 'browser',
      format: 'esm',
      outDir,
      config: false,
      tsconfig: 'tests/consumer/tsconfig.json',
      noExternal: ['spacetimedb'],
      silent: true,
    });
  } finally {
    await rm(outDir, { recursive: true, force: true });
  }
});
