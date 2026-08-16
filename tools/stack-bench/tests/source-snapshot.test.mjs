import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { hashAppSource, restoreAppSource, seedAppSource, snapshotAppSource } from '../source-snapshot.mjs';

const put = (path, content) => {
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
  put(join(app, '.session-fix-l1.json'), '{}\n');
  put(join(app, 'ui', 'dist', 'bundle.js'), 'compiled\n');

  const watchedDirectoryIdentity = statSync(client).ino;
  snapshotAppSource(app, snapshot);

  assert.equal(existsSync(join(snapshot, 'backend', 'spacetimedb', 'node_modules')), false);
  assert.equal(existsSync(join(snapshot, 'BUG_REPORT.md')), false);
  assert.equal(existsSync(join(snapshot, 'ui', 'dist')), false);

  put(join(client, 'App.tsx'), 'export const value = "bad fix";\n');
  put(join(client, 'introduced.ts'), 'remove me\n');
  put(join(app, 'functions', 'handler.mjs'), 'broken\n');
  put(join(app, 'new-layout', 'bad-source.js'), 'remove me\n');
  put(join(app, 'new-layout', 'node_modules', 'installed', 'index.js'), 'keep me\n');
  put(join(app, 'ui', 'dist', 'stale.js'), 'remove me\n');
  put(join(app, 'BUG_REPORT.md'), 'latest harness report\n');

  restoreAppSource(snapshot, app);

  assert.equal(readFileSync(join(client, 'App.tsx'), 'utf8'), 'export const value = "good";\n');
  assert.equal(readFileSync(join(app, 'functions', 'handler.mjs'), 'utf8'), 'export default "original";\n');
  assert.equal(existsSync(join(client, 'introduced.ts')), false);
  assert.equal(existsSync(join(app, 'new-layout', 'bad-source.js')), false);
  assert.equal(readFileSync(join(app, 'new-layout', 'node_modules', 'installed', 'index.js'), 'utf8'), 'keep me\n');
  assert.equal(readFileSync(join(app, 'backend', 'spacetimedb', 'node_modules', 'dep', 'index.js'), 'utf8'), 'dependency\n');
  assert.equal(existsSync(join(app, 'ui', 'dist')), false);
  assert.equal(readFileSync(join(app, 'BUG_REPORT.md'), 'utf8'), 'latest harness report\n');
  assert.equal(statSync(client).ino, watchedDirectoryIdentity, 'restore replaced a watched source directory');
});

test('source seeding copies arbitrary layouts without dependencies or harness evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-seed-'));
  const source = join(root, 'source');
  const target = join(root, 'target');
  put(join(source, 'unconventional', 'api', 'main.go'), 'package main\n');
  put(join(source, 'unconventional', 'node_modules', 'dep.js'), 'dependency\n');
  put(join(source, '.prompt-build-l1.md'), 'private harness prompt\n');

  seedAppSource(source, target);

  assert.equal(readFileSync(join(target, 'unconventional', 'api', 'main.go'), 'utf8'), 'package main\n');
  assert.equal(existsSync(join(target, 'unconventional', 'node_modules')), false);
  assert.equal(existsSync(join(target, '.prompt-build-l1.md')), false);
});

test('source identity matches preserved bytes and ignores dependencies and harness evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-identity-'));
  const app = join(root, 'app');
  const snapshot = join(root, 'snapshot');
  try {
    put(join(app, 'src', 'app.ts'), 'export const value = 1;\n');
    put(join(app, 'node_modules', 'dep', 'index.js'), 'dependency v1\n');
    put(join(app, 'stack-bench', 'bundle.json'), '{}\n');
    put(join(app, 'BUG_REPORT.md'), 'private evidence\n');
    const first = hashAppSource(app);
    snapshotAppSource(app, snapshot);
    assert.equal(hashAppSource(snapshot).sha256, first.sha256);
    put(join(app, 'node_modules', 'dep', 'index.js'), 'dependency v2\n');
    put(join(app, 'stack-bench', 'bundle.json'), '{"changed":true}\n');
    assert.equal(hashAppSource(app).sha256, first.sha256);
    put(join(app, 'src', 'app.ts'), 'export const value = 2;\n');
    assert.notEqual(hashAppSource(app).sha256, first.sha256);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
