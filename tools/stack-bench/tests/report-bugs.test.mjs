import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { writeArtifact } from '../src/evidence/artifacts.mjs';
import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';

const CLI = join(import.meta.dirname, '..', 'commands', 'report-bugs.mjs');

function writeGrade(app, status, summary, { grading = join(app, 'stack-bench'),
  feature = 'Accounts', points = 1 } = {}) {
  mkdirSync(grading, { recursive: true });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const evidence = createCheckEvidence({ status, code: status === 'passed' ? 'completed' : 'test_result',
    phase: 'assertion', summary, startedAtMs: 3, completedAtMs: 4 });
  writeArtifact(join(grading, 'grading-features.json'), {
    kind: 'grade', id: 'repair-selection-grade', identities: {},
    payload: {
      total: status === 'passed' ? points : 0,
      max: status === 'inconclusive' || status === 'harness_failure' ? 0 : points,
      features: [{ id: 1, name: feature, score: status === 'passed' ? points : 0,
        max: status === 'inconclusive' || status === 'harness_failure' ? 0 : points,
        setupEvidence, criteria: [{ id: 'owner', desc: 'Only the owner can edit', points, evidence }] }],
    },
  });
}

test('repair report selection follows typed evidence even when prose claims the opposite', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-selection-'));
  try {
    const harnessApp = join(root, 'harness');
    writeGrade(harnessApp, 'harness_failure', 'FAILED: the generated app is definitely broken');
    const skipped = spawnSync(process.execPath, [CLI, '--app', harnessApp], { encoding: 'utf8' });
    assert.equal(skipped.status, 3, skipped.stderr);
    assert.equal(existsSync(join(harnessApp, 'BUG_REPORT.md')), false);

    const failedApp = join(root, 'failed');
    writeGrade(failedApp, 'failed', 'INCONCLUSIVE: this wording must not suppress repair');
    const reported = spawnSync(process.execPath, [CLI, '--app', failedApp], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    assert.match(readFileSync(join(failedApp, 'BUG_REPORT.md'), 'utf8'),
      /INCONCLUSIVE: this wording must not suppress repair/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('expected failures enter repairs while observed-only failures stay isolated', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-treatment-repair-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'durability was expected but state was lost',
      { feature: 'State durability' });
    writeGrade(app, 'failed', 'observed-only failure must not enter repair', {
      grading: join(root, 'run', 'first-build-l1-observed'), feature: 'Observed behavior',
    });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /State durability/);
    assert.match(repair, /durability was expected but state was lost/);
    assert.doesNotMatch(repair, /observed-only failure must not enter repair|Observed behavior/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('zero-point test-development failures never enter repair feedback', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-zero-point-repair-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'candidate behavior failed',
      { feature: 'Candidate concurrency check', points: 0 });
    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 3, reported.stderr);
    assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('application setup failures become actionable repair feedback without criterion results', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-setup-repair-'));
  try {
    const app = join(root, 'app');
    const grading = join(app, 'stack-bench');
    mkdirSync(grading, { recursive: true });
    writeArtifact(join(grading, 'bundle.json'), {
      kind: 'grade_bundle', id: 'setup-failure-bundle', identities: {},
      payload: {
        definitionSchemaVersion: 1, recipeRelease: null, calibration: null,
        label: 'postgres-l1', track: 'ecommerce', backend: 'postgres', url: 'http://app',
        app, level: 1, observation: 'scored', suites: {},
        totals: { score: 0, max: 58, dirty: false, contractPass: null, regression: null },
        error: 'database reset failed: server/package.json has no dev or start script',
        outcome: { kind: 'app_failure', phase: 'application-restart',
          reason: 'database reset failed: server/package.json has no dev or start script',
          appFailures: ['application-restart'] },
        selection: null,
      },
    });
    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Application setup/);
    assert.match(repair, /repeatable command that starts its server/);
    assert.match(repair, /no dev or start script/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
