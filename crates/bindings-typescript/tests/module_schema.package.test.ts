import { execFileSync } from 'node:child_process';
import { copyFile, mkdtemp, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { build } from 'tsup';
import { createRequire } from 'node:module';
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

// Regression: Node bundling once pulled in both SDK source and built SDK classes.
// `createClient` then rejected the schema with "Expected a module with a schema
// default export" because its instanceof check used the other copy of Schema.
// Bundle a client and start it in Node; constructing its client must succeed.
test('module schemas share SDK classes in Node bundles', async () => {
  // Use the separate client's directory so esbuild picks up its tsconfig,
  // rather than the SDK's configuration for tests against its own source.
  const outDir = await mkdtemp(
    fileURLToPath(
      new URL('../case-conversion-test-client/.module-schema-', import.meta.url)
    )
  );
  try {
    const outfile = join(outDir, 'consumer.cjs');
    const entry = join(outDir, 'consumer.ts');
    await copyFile('tests/consumer/module-schema-browser.ts', entry);
    const require = createRequire(
      new URL('../case-conversion-test-client/package.json', import.meta.url)
    );
    execFileSync(
      process.execPath,
      [
        require.resolve('esbuild/bin/esbuild'),
        entry,
        '--bundle',
        '--platform=node',
        '--format=cjs',
        `--outfile=${outfile}`,
      ],
      { stdio: 'pipe' }
    );
    execFileSync(process.execPath, [outfile], { stdio: 'pipe' });
  } finally {
    await rm(outDir, { recursive: true, force: true });
  }
});
