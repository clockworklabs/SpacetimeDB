import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { assertAppSourceIdentity, hashAppSource, resetAppToSource, restoreAppSource, seedAppSource,
  snapshotAppSource } from '../src/runtime/source-snapshot.js';

const put = (path: string, content: string): void => {
  mkdirSync(join(path, '..'), { recursive: true });
  writeFileSync(path, content);
};

test('source rollback is layout-independent and preserves watched directories and nested dependencies', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-snapshot-'));
  const app = join(root, 'app');
  const snapshot = join(root, 'snapshot');
  const client = join(app, 'ui', 'src');
  put(join(client, 'App.tsx'), 'export const value = "good";\n');
  put(join(app, 'functions', 'handler.mjs'), 'export default "original";\n');
  put(join(app, 'backend', 'spacetimedb', 'node_modules', 'dep', 'index.js'), 'dependency\n');
  put(join(app, 'BUG_REPORT.md'), 'current harness report\n');
  put(join(app, 'bug-report-quality.json'), '{}\n');
  put(join(app, '.read-guard-settings.json'), '{}\n');
  put(join(app, '.sandbox-settings.json'), '{}\n');
  put(join(app, '.stack-bench-backend'), 'mongodb\n');
  put(join(app, '.prompt-build-l1.md'), 'prompt\n');
  put(join(app, '.session-fix-l1.json'), '{}\n');
  put(join(app, 'ui', 'dist', 'bundle.js'), 'compiled\n');
  put(join(app, 'ui', 'src', 'module_bindings', 'index.ts'), 'generated\n');
  put(join(app, 'server.log'), 'server output\n');
  put(join(app, 'ui', 'vite.log'), 'client output\n');
  put(join(app, 'src', 'stack-bench', 'runtime.ts'), 'export const owned = true;\n');
  put(join(app, 'src', 'server.log'), 'model-authored input\n');

  const watchedDirectoryIdentity = statSync(client).ino;
  snapshotAppSource(app, snapshot);

  assert.equal(existsSync(join(snapshot, 'backend', 'spacetimedb', 'node_modules')), false);
  assert.equal(existsSync(join(snapshot, 'BUG_REPORT.md')), false);
  for (const file of ['bug-report-quality.json', '.read-guard-settings.json',
    '.sandbox-settings.json', '.stack-bench-backend', '.prompt-build-l1.md',
    '.session-fix-l1.json']) {
    assert.equal(existsSync(join(snapshot, file)), true);
  }
  assert.equal(existsSync(join(snapshot, 'ui', 'dist')), false);
  assert.equal(existsSync(join(snapshot, 'ui', 'src', 'module_bindings')), true);
  assert.equal(existsSync(join(snapshot, 'server.log')), false);
  assert.equal(existsSync(join(snapshot, 'ui', 'vite.log')), true);
  assert.equal(existsSync(join(snapshot, 'src', 'stack-bench', 'runtime.ts')), true);
  assert.equal(existsSync(join(snapshot, 'src', 'server.log')), true);

  put(join(client, 'App.tsx'), 'export const value = "bad fix";\n');
  put(join(client, 'introduced.ts'), 'remove me\n');
  put(join(app, 'functions', 'handler.mjs'), 'broken\n');
  put(join(app, 'new-layout', 'bad-source.js'), 'remove me\n');
  put(join(app, 'new-layout', 'node_modules', 'installed', 'index.js'), 'keep me\n');
  put(join(app, 'ui', 'dist', 'stale.js'), 'remove me\n');
  put(join(app, 'ui', 'src', 'module_bindings', 'new.ts'), 'stale binding\n');
  put(join(app, 'BUG_REPORT.md'), 'latest harness report\n');
  put(join(app, 'bug-report-quality.json'), '{"latest":true}\n');
  put(join(app, 'server.log'), 'new server output\n');
  put(join(app, 'ui', 'vite.log'), 'new client output\n');

  restoreAppSource(snapshot, app);

  assert.equal(readFileSync(join(client, 'App.tsx'), 'utf8'), 'export const value = "good";\n');
  assert.equal(readFileSync(join(app, 'functions', 'handler.mjs'), 'utf8'), 'export default "original";\n');
  assert.equal(existsSync(join(client, 'introduced.ts')), false);
  assert.equal(existsSync(join(app, 'new-layout', 'bad-source.js')), false);
  assert.equal(readFileSync(join(app, 'new-layout', 'node_modules', 'installed', 'index.js'), 'utf8'), 'keep me\n');
  assert.equal(readFileSync(join(app, 'backend', 'spacetimedb', 'node_modules', 'dep', 'index.js'), 'utf8'), 'dependency\n');
  assert.equal(existsSync(join(app, 'ui', 'dist')), false);
  assert.equal(existsSync(join(app, 'ui', 'src', 'module_bindings')), true);
  assert.equal(readFileSync(join(app, 'BUG_REPORT.md'), 'utf8'), 'latest harness report\n');
  assert.equal(readFileSync(join(app, 'bug-report-quality.json'), 'utf8'), '{}\n');
  assert.equal(readFileSync(join(app, 'server.log'), 'utf8'), 'new server output\n');
  assert.equal(readFileSync(join(app, 'ui', 'vite.log'), 'utf8'), 'client output\n');
  assert.equal(statSync(client).ino, watchedDirectoryIdentity, 'restore replaced a watched source directory');
});

test('source seeding copies arbitrary layouts without dependencies or repair evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-seed-'));
  const source = join(root, 'source');
  const target = join(root, 'target');
  put(join(source, 'unconventional', 'api', 'main.go'), 'package main\n');
  put(join(source, 'unconventional', 'node_modules', 'dep.js'), 'dependency\n');
  put(join(source, '.prompt-build-l1.md'), 'model-authored file\n');

  seedAppSource(source, target);

  assert.equal(readFileSync(join(target, 'unconventional', 'api', 'main.go'), 'utf8'), 'package main\n');
  assert.equal(existsSync(join(target, 'unconventional', 'node_modules')), false);
  assert.equal(readFileSync(join(target, '.prompt-build-l1.md'), 'utf8'), 'model-authored file\n');
});

test('source snapshots exclude package and browser tool caches', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-caches-'));
  const app = join(root, 'app');
  const snapshot = join(root, 'snapshot');
  try {
    put(join(app, 'src', 'app.ts'), 'export const app = true;\n');
    put(join(app, 'client', 'tsconfig.tsbuildinfo'), 'compiler cache\n');
    for (const directory of ['.apt', '.cache', '.debroot', '.libs', '.pw-browsers', '.pwcache']) {
      put(join(app, directory, 'tool-artifact'), 'not application source\n');
    }
    const before = hashAppSource(app);
    snapshotAppSource(app, snapshot);
    assert.equal(readFileSync(join(snapshot, 'src', 'app.ts'), 'utf8'),
      'export const app = true;\n');
    assert.equal(existsSync(join(snapshot, 'client', 'tsconfig.tsbuildinfo')), false);
    restoreAppSource(snapshot, app);
    assert.equal(existsSync(join(app, 'client', 'tsconfig.tsbuildinfo')), false);
    for (const directory of ['.apt', '.cache', '.debroot', '.libs', '.pw-browsers', '.pwcache']) {
      assert.equal(existsSync(join(snapshot, directory)), false);
    }
    assert.equal(hashAppSource(snapshot).sha256, before.sha256);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('clean source reset removes all runtime state and preserves only root git metadata', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-clean-source-reset-'));
  const app = join(root, 'app');
  const snapshot = join(root, 'snapshot');
  try {
    put(join(app, 'src', 'app.ts'), 'export const value = 1;\n');
    put(join(app, 'src', 'stack-bench', 'runtime.ts'), 'export const owned = true;\n');
    put(join(app, 'src', 'server.log'), 'model-authored input\n');
    put(join(app, '.git', 'HEAD'), 'before-reset\n');
    snapshotAppSource(app, snapshot);

    put(join(app, 'src', 'app.ts'), 'export const value = 2;\n');
    put(join(app, 'introduced.ts'), 'remove me\n');
    put(join(app, '.git', 'HEAD'), 'preserved\n');
    put(join(app, 'node_modules', 'package', 'index.js'), 'patched dependency\n');
    put(join(app, 'server', 'node_modules', 'package', 'index.js'), 'nested dependency\n');
    put(join(app, 'dist', 'server.js'), 'generated\n');
    put(join(app, 'client', '.vite', 'bundle.js'), 'generated\n');
    put(join(app, 'client', '.cache', 'entry'), 'cached\n');
    put(join(app, 'client', 'src', 'module_bindings', 'index.ts'), 'generated binding\n');
    put(join(app, 'server', 'tsconfig.tsbuildinfo'), 'compiler cache\n');
    put(join(app, 'stack-bench', 'bundle.json'), '{"private":true}\n');
    put(join(app, 'BUG_REPORT.md'), 'repair evidence\n');
    put(join(app, 'server.log'), 'runtime log\n');
    put(join(app, 'client.log'), 'runtime log\n');
    put(join(app, 'vite.log'), 'runtime log\n');

    resetAppToSource(snapshot, app);

    assert.equal(readFileSync(join(app, 'src', 'app.ts'), 'utf8'), 'export const value = 1;\n');
    assert.equal(readFileSync(join(app, 'src', 'stack-bench', 'runtime.ts'), 'utf8'),
      'export const owned = true;\n');
    assert.equal(readFileSync(join(app, 'src', 'server.log'), 'utf8'), 'model-authored input\n');
    assert.equal(readFileSync(join(app, '.git', 'HEAD'), 'utf8'), 'preserved\n');
    for (const path of ['introduced.ts', 'node_modules', join('server', 'node_modules'),
      'dist', join('client', '.vite'), join('client', '.cache'),
      join('client', 'src', 'module_bindings'), join('server', 'tsconfig.tsbuildinfo'),
      'stack-bench', 'BUG_REPORT.md', 'server.log', 'client.log', 'vite.log']) {
      assert.equal(existsSync(join(app, path)), false, `${path} survived the clean source reset`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('source identity matches preserved bytes and ignores dependencies and harness evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-identity-'));
  const app = join(root, 'app');
  const snapshot = join(root, 'snapshot');
  try {
    put(join(app, 'src', 'app.ts'), 'export const value = 1;\n');
    put(join(app, 'node_modules', 'dep', 'index.js'), 'dependency v1\n');
    put(join(app, 'stack-bench', 'bundle.json'), '{}\n');
    put(join(app, 'client', 'src', 'module_bindings', 'index.ts'), 'generated v1\n');
    put(join(app, 'BUG_REPORT.md'), 'private evidence\n');
    put(join(app, 'bug-report-quality.json'), '{}\n');
    put(join(app, 'server.log'), 'server output v1\n');
    put(join(app, 'client', 'vite.log'), 'client output v1\n');
    put(join(app, 'src', 'stack-bench', 'runtime.ts'), 'export const owned = 1;\n');
    put(join(app, 'src', 'server.log'), 'model-authored input v1\n');
    const first = hashAppSource(app);
    assert.deepEqual(assertAppSourceIdentity(app, first.sha256), first);
    snapshotAppSource(app, snapshot);
    assert.equal(hashAppSource(snapshot).sha256, first.sha256);
    put(join(app, 'node_modules', 'dep', 'index.js'), 'dependency v2\n');
    put(join(app, 'stack-bench', 'bundle.json'), '{"changed":true}\n');
    put(join(app, 'client', 'src', 'module_bindings', 'index.ts'), 'generated v2\n');
    put(join(app, 'server.log'), 'server output v2\n');
    assert.equal(hashAppSource(app).sha256, first.sha256);
    put(join(app, 'client', 'vite.log'), 'client output v2\n');
    assert.notEqual(hashAppSource(app).sha256, first.sha256);
    put(join(app, 'client', 'vite.log'), 'client output v1\n');
    put(join(app, 'src', 'stack-bench', 'runtime.ts'), 'export const owned = 2;\n');
    assert.notEqual(hashAppSource(app).sha256, first.sha256);
    put(join(app, 'src', 'stack-bench', 'runtime.ts'), 'export const owned = 1;\n');
    put(join(app, 'src', 'server.log'), 'model-authored input v2\n');
    assert.notEqual(hashAppSource(app).sha256, first.sha256);
    put(join(app, 'src', 'server.log'), 'model-authored input v1\n');
    put(join(app, 'src', 'app.ts'), 'export const value = 2;\n');
    assert.notEqual(hashAppSource(app).sha256, first.sha256);
    assert.throws(() => assertAppSourceIdentity(app, first.sha256, 'restored mutation source'),
      /restored mutation source hash .* does not match/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
