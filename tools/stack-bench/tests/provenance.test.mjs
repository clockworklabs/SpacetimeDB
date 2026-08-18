import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashDirectory, hashFiles, hashRubric, sessionProvenance, sha256 } from '../src/evidence/provenance.mjs';

test('file-set hashes bind both relative names and exact bytes', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-provenance-'));
  try {
    mkdirSync(join(root, 'a'));
    const one = join(root, 'a', 'one.txt');
    const two = join(root, 'two.txt');
    writeFileSync(one, 'ab');
    writeFileSync(two, 'c');
    const first = hashFiles([two, one], { base: root });
    assert.deepEqual(first.files, ['a/one.txt', 'two.txt']);
    assert.equal(first.sha256, hashFiles([one, two], { base: root }).sha256);
    writeFileSync(two, 'changed');
    assert.notEqual(first.sha256, hashFiles([one, two], { base: root }).sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('rubric hash changes with points but not scenario mechanics', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-provenance-'));
  const spec = join(root, 'spec.json');
  try {
    const write = (points, within) => writeFileSync(spec, JSON.stringify({ features: [{ id: 1,
      criteria: [{ id: '1a', points, steps: [{ do: 'expect', within }] }] }] }));
    write(2, 100);
    const initial = hashRubric([spec], { base: root });
    write(2, 500);
    assert.equal(initial.sha256, hashRubric([spec], { base: root }).sha256);
    write(3, 500);
    assert.notEqual(initial.sha256, hashRubric([spec], { base: root }).sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('directory hashes can exclude reproducible build output', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-provenance-'));
  try {
    mkdirSync(join(root, 'src'));
    mkdirSync(join(root, 'dist'));
    writeFileSync(join(root, 'src', 'index.ts'), 'source');
    writeFileSync(join(root, 'dist', 'index.js'), 'first build');
    const source = () => hashDirectory(root, { exclude: name => name === 'dist' || name.startsWith('dist/') });
    const first = source();
    writeFileSync(join(root, 'dist', 'index.js'), 'second build');
    assert.equal(first.sha256, source().sha256);
    writeFileSync(join(root, 'src', 'index.ts'), 'changed source');
    assert.notEqual(first.sha256, source().sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('session provenance identifies every comparison-defining input', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-provenance-'));
  try {
    const manifest = join(root, 'track.json');
    const spec = join(root, 'scenario.json');
    writeFileSync(manifest, '{}');
    writeFileSync(spec, JSON.stringify({ features: [] }));
    const result = sessionProvenance({ prompt: 'prompt', skillsText: 'skill',
      contractText: 'contract', scenarioPaths: [spec], trackDir: root,
      trackManifestPath: manifest });
    assert.equal(result.promptSha256, sha256('prompt'));
    assert.equal(result.skillsSha256, sha256('skill'));
    assert.equal(result.contractSha256, sha256('contract'));
    assert.equal(result.scenarioFiles[0], 'scenario.json');
    assert.match(result.rubric.sha256, /^[0-9a-f]{64}$/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
