import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createDependencyManifest, initializeDependencyVolume, manifestSha256,
  verifyDependencyTree } from '../appliance/dependency-volume.mjs';

function fixture(t) {
  const root = join(tmpdir(), `stack-bench-deps-${process.pid}-${Math.random().toString(16).slice(2)}`);
  const source = join(root, 'source');
  const target = join(root, 'target');
  mkdirSync(join(source, 'bindings-typescript'), { recursive: true });
  writeFileSync(join(source, 'bindings-typescript', 'package.json'), '{"name":"sdk"}\n');
  writeFileSync(join(source, 'spacetimedb-cli'), 'cli');
  chmodSync(join(source, 'spacetimedb-cli'), 0o755);
  t.after(() => rmSync(root, { recursive: true, force: true }));
  return { source, target };
}

test('dependency volume initialization is exact and idempotent for one release', t => {
  const { source, target } = fixture(t);
  const manifest = createDependencyManifest(source);
  const first = initializeDependencyVolume({ source, target, manifest });
  assert.equal(first.initialized, true);
  assert.equal(first.manifestSha256, manifestSha256(manifest));
  assert.deepEqual(verifyDependencyTree(target, manifest, { allowMarker: true }), {
    manifestSha256: manifestSha256(manifest), files: 2,
  });
  assert.equal(initializeDependencyVolume({ source, target, manifest }).initialized, false);
});

test('dependency volume refuses unmarked, stale, changed and wrong-release content', t => {
  const { source, target } = fixture(t);
  const manifest = createDependencyManifest(source);
  mkdirSync(target);
  writeFileSync(join(target, 'foreign'), 'x');
  assert.throws(() => initializeDependencyVolume({ source, target, manifest }), /no release marker/);

  const clean = join(target, '..', 'clean');
  initializeDependencyVolume({ source, target: clean, manifest });
  writeFileSync(join(clean, 'spacetimedb-cli'), 'changed');
  assert.throws(() => verifyDependencyTree(clean, manifest, { allowMarker: true }), /does not match/);
  assert.throws(() => initializeDependencyVolume({ source, target: clean, manifest }), /does not match/);

  const marker = JSON.parse(readFileSync(join(clean, '.stack-bench-release-deps.json'), 'utf8'));
  marker.manifestSha256 = '0'.repeat(64);
  chmodSync(join(clean, '.stack-bench-release-deps.json'), 0o644);
  writeFileSync(join(clean, '.stack-bench-release-deps.json'), JSON.stringify(marker));
  assert.throws(() => initializeDependencyVolume({ source, target: clean, manifest }), /different release/);
});
