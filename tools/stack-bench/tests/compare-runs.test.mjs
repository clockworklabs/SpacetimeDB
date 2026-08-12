import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { writeArtifact } from '../artifacts.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const CLI = join(import.meta.dirname, '..', 'compare-runs.mjs');
const RECIPE = 'a'.repeat(64);

function runDirectory(root, name, { selection = 'b'.repeat(64), identified = true } = {}) {
  const dir = join(root, name);
  const grading = join(dir, 'grading');
  mkdirSync(grading, { recursive: true });
  writeArtifact(join(grading, 'grading-features.json'), {
    kind: 'grade', id: `${name}-grade`,
    identities: identified ? { recipe: { id: 'recipe', version: '1.0.0', sha256: RECIPE } } : {},
    payload: {
      total: 1, max: 1,
      selection: identified ? { sha256: selection, checks: [] } : null,
      features: [{ id: 1, name: 'feature', setupEvidence: createCheckEvidence({ status: 'passed',
        code: 'completed', phase: 'setup', startedAtMs: 1, completedAtMs: 2 }),
      criteria: [{ id: 'works', stableKey: 'pack.group.works', points: 1,
        evidence: createCheckEvidence({ status: 'passed', code: 'completed', phase: 'assertion',
          startedAtMs: 1, completedAtMs: 2 }) }] }],
    },
  });
  return dir;
}

function typedRunDirectory(root, name, status, summary) {
  const dir = join(root, name);
  const grading = join(dir, 'grading');
  mkdirSync(grading, { recursive: true });
  const evidence = createCheckEvidence({ status, code: status === 'passed' ? 'completed' : 'test_result',
    phase: 'assertion', summary, startedAtMs: 1, completedAtMs: 2 });
  writeArtifact(join(grading, 'grading-features.json'), {
    kind: 'grade', id: `${name}-grade`,
    identities: { recipe: { id: 'recipe', version: '1.0.0', sha256: RECIPE } },
    payload: {
      total: status === 'passed' ? 1 : 0, max: 1,
      selection: { sha256: 'b'.repeat(64), checks: [] },
      features: [{ id: 1, setupEvidence: createCheckEvidence({ status: 'passed', code: 'completed',
        phase: 'setup', startedAtMs: 1, completedAtMs: 2 }),
      criteria: [{ id: 'works', stableKey: 'pack.group.works', points: 1, evidence }] }],
    },
  });
  return dir;
}

test('run comparison requires matching recipe and selection identities', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-compare-'));
  try {
    const first = runDirectory(root, 'first');
    const same = runDirectory(root, 'same');
    const different = runDirectory(root, 'different', { selection: 'c'.repeat(64) });
    const comparable = spawnSync(process.execPath, [CLI, first, same], { encoding: 'utf8' });
    assert.equal(comparable.status, 0, comparable.stderr);
    assert.match(comparable.stdout, /Comparable criteria: 1 of 1/);
    const mismatch = spawnSync(process.execPath, [CLI, first, different], { encoding: 'utf8' });
    assert.notEqual(mismatch.status, 0);
    assert.match(`${mismatch.stdout}${mismatch.stderr}`, /different recipe or selection identities/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('unidentified scopes are always refused', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-compare-'));
  try {
    const first = runDirectory(root, 'first');
    const legacy = runDirectory(root, 'legacy', { identified: false });
    const refused = spawnSync(process.execPath, [CLI, first, legacy], { encoding: 'utf8' });
    assert.notEqual(refused.status, 0);
    assert.match(`${refused.stdout}${refused.stderr}`, /cannot prove comparable run scope/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('run comparison uses typed status even when summary wording is misleading', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-compare-'));
  try {
    const first = typedRunDirectory(root, 'first', 'passed', null);
    const failed = typedRunDirectory(root, 'failed', 'failed', 'INCONCLUSIVE: misleading prose');
    const compared = spawnSync(process.execPath, [CLI, first, failed], { encoding: 'utf8' });
    assert.equal(compared.status, 0, compared.stderr);
    assert.match(compared.stdout, /Comparable criteria: 1 of 1/);
    assert.match(compared.stdout, /Disagreements on comparable criteria: 1/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
