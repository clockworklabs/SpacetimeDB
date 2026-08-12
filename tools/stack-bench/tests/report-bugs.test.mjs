import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { writeArtifact } from '../artifacts.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const CLI = join(import.meta.dirname, '..', 'report-bugs.mjs');

function writeGrade(app, status, summary) {
  const grading = join(app, 'stack-bench');
  mkdirSync(grading, { recursive: true });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const evidence = createCheckEvidence({ status, code: status === 'passed' ? 'completed' : 'test_result',
    phase: 'assertion', summary, startedAtMs: 3, completedAtMs: 4 });
  writeArtifact(join(grading, 'grading-features.json'), {
    kind: 'grade', id: 'repair-selection-grade', identities: {},
    payload: {
      total: status === 'passed' ? 1 : 0,
      max: status === 'inconclusive' || status === 'harness_failure' ? 0 : 1,
      features: [{ id: 1, name: 'Accounts', score: status === 'passed' ? 1 : 0,
        max: status === 'inconclusive' || status === 'harness_failure' ? 0 : 1,
        setupEvidence, criteria: [{ id: 'owner', desc: 'Only the owner can edit', points: 1, evidence }] }],
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
