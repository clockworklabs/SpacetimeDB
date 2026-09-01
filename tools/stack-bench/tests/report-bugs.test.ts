import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { writeArtifact } from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';

const CLI = join(STACK_BENCH_ROOT, 'dist', 'commands', 'report-bugs.js');

type EvidenceStatus = 'passed' | 'failed' | 'inconclusive' | 'harness_failure';

interface WriteGradeOptions {
  grading?: string;
  feature?: string;
  points?: number;
  criterion?: string;
  stableKey?: string;
  file?: string;
  url?: string;
  consoleErrors?: string[];
  evidence?: ReturnType<typeof createCheckEvidence>;
}

function writeGrade(app: string, status: EvidenceStatus, summary: string,
  { grading = join(app, 'stack-bench'), feature = 'Accounts', points = 1,
    criterion = 'owner', stableKey = criterion, file = 'grading-features.json',
    url = 'http://app', consoleErrors = [], evidence: suppliedEvidence }:
    WriteGradeOptions = {}): void {
  mkdirSync(grading, { recursive: true });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const evidence = suppliedEvidence ?? createCheckEvidence({ status,
    code: status === 'passed' ? 'completed' : 'test_result', phase: 'assertion', summary,
    startedAtMs: 3, completedAtMs: 4 });
  writeArtifact(join(grading, file), {
    kind: 'grade', id: 'repair-selection-grade', identities: {},
    payload: {
      url,
      total: status === 'passed' ? points : 0,
      max: status === 'inconclusive' || status === 'harness_failure' ? 0 : points,
      features: [{ id: 1, name: feature, score: status === 'passed' ? points : 0,
        max: status === 'inconclusive' || status === 'harness_failure' ? 0 : points,
        setupEvidence, consoleErrors, criteria: [{ id: criterion, stableKey,
          desc: `Expected ${criterion}`, points, evidence }] }],
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

test('repair feedback includes actionable runtime evidence without private artifact names', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-diagnostics-'));
  try {
    const app = join(root, 'app');
    const evidence = createCheckEvidence({
      status: 'failed', code: 'test_result', phase: 'assertion', actor: 'buyer',
      summary: 'cart total was wrong', observation: { value: 9 }, expected: { value: 12 },
      startedAtMs: 3, completedAtMs: 4,
      actions: [{ actor: 'buyer', evidence: {
        schemaVersion: 1, action: { id: 'expectNumber', version: '1.0.0' },
        status: 'failed', type: 'browser-number', code: 'application_failure', phase: 'execute',
        summary: 'cart total was wrong', observation: { value: 9 }, expected: { value: 12 },
        retryable: false, timing: { startedAtMs: 3, completedAtMs: 4, durationMs: 1,
          deadlineMs: 5_000 }, attachments: [], sensitivity: [],
      } }],
      attachments: [{ kind: 'screenshot', ref: 'failure-buyer.png' }],
    });
    writeGrade(app, 'failed', 'cart total was wrong', {
      feature: 'Cart', criterion: 'total', stableKey: 'private.check.cart.total',
      url: 'http://app/cart', consoleErrors: ['POST /api/cart returned HTTP 500'], evidence,
    });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Actor\/session:\*\* buyer/);
    assert.match(repair, /Expected:\*\* 12/);
    assert.match(repair, /Actual:\*\* 9/);
    assert.match(repair, /Application URL:\*\* `http:\/\/app\/cart`/);
    assert.doesNotMatch(repair, /failure-buyer\.png/);
    assert.match(repair, /Console or network errors:[\s\S]*HTTP 500/);
    assert.doesNotMatch(repair, /expect number|private\.check|Stack Bench|grader|criterion/i);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback refuses internal evaluation language', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-disclosure-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'the Stack Bench test failed');
    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 2);
    assert.match(reported.stderr, /contains internal language/);
    assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('dependency repair feedback contains only checks selected for that feature', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-check-selection-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'account ownership failed', {
      criterion: 'owner', stableKey: 'check.accounts.owner',
      file: 'grading-accounts.json', feature: 'Accounts',
    });
    writeGrade(app, 'failed', 'catalog search failed', {
      criterion: 'search', stableKey: 'check.catalog.search',
      file: 'grading-catalog.json', feature: 'Catalog',
    });
    const reported = spawnSync(process.execPath, [CLI, '--app', app,
      '--checks-json', JSON.stringify(['check.accounts.owner']),
      '--history-json', JSON.stringify([{ round: 1, result: 'incomplete',
        remainingFailures: ['check.accounts.owner', 'check.catalog.search'] }])],
    { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Accounts|account ownership failed/);
    assert.match(repair, /Earlier work/);
    assert.doesNotMatch(repair, /remaining:/);
    assert.doesNotMatch(repair, /Catalog|catalog search failed/);
    assert.doesNotMatch(repair, /check\.catalog\.search/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('a vague-only report does not authorize a paid repair', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-vague-repair-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', '');
    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 4, reported.stderr);
    assert.match(reported.stdout, /No actionable failures/);
    assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
    assert.equal(existsSync(join(app, 'stack-bench', 'bug-report-quality.json')), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair metadata stays in the harness evidence directory and does not change source identity', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-metadata-'));
  try {
    const app = join(root, 'app');
    mkdirSync(app, { recursive: true });
    writeGrade(app, 'failed', 'the owner check still failed');
    const before = hashAppSource(app).sha256;

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });

    assert.equal(reported.status, 0, reported.stderr);
    assert.equal(existsSync(join(app, 'bug-report-quality.json')), false);
    assert.equal(existsSync(join(app, 'stack-bench', 'bug-report-quality.json')), true);
    assert.equal(hashAppSource(app).sha256, before);
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
    assert.match(repair, /must provide \/app\/start\.sh/);
    assert.match(repair, /no dev or start script/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('contract feedback reports the clean-state observation without claiming the element exists', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-contract-repair-'));
  try {
    const app = join(root, 'app');
    const grading = join(app, 'stack-bench');
    mkdirSync(grading, { recursive: true });
    writeArtifact(join(grading, 'contract-lint.json'), {
      kind: 'contract_lint', id: 'contract-repair-lint', identities: {},
      payload: {
        label: 'spacetime-l1', url: 'http://app', level: 1, pass: false,
        counts: { lintable: 2, pass: 0, fail: 1, blocked: 1, scenario: 1 },
        results: [
          { id: 'review-average', status: 'FAIL',
            detail: 'no element matching [data-testid="review-average"] became visible — expected: the item average as a number' },
          { id: 'cart-panel', status: 'BLOCKED', detail: 'earlier core flow step failed' },
        ],
      },
    });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /clean application state/);
    assert.match(repair, /review-average/);
    assert.match(repair, /no element matching/);
    assert.doesNotMatch(repair, /These elements exist/);
    assert.doesNotMatch(repair, /cart-panel/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('dependency repair feedback contains only controls selected for that feature', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-control-selection-'));
  try {
    const app = join(root, 'app');
    const grading = join(app, 'stack-bench');
    mkdirSync(grading, { recursive: true });
    writeArtifact(join(grading, 'contract-lint.json'), {
      kind: 'contract_lint', id: 'contract-repair-lint', identities: {},
      payload: {
        label: 'postgres-l1', url: 'http://app', level: 1, pass: false,
        counts: { lintable: 2, pass: 0, fail: 2, blocked: 0, scenario: 1 },
        results: [
          { id: 'account-menu', status: 'FAIL', detail: 'account menu missing' },
          { id: 'catalog-search', status: 'FAIL', detail: 'catalog search missing' },
        ],
      },
    });
    const reported = spawnSync(process.execPath, [CLI, '--app', app,
      '--checks-json', JSON.stringify(['check.accounts.owner']),
      '--controls-json', JSON.stringify(['account-menu'])], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /account-menu/);
    assert.doesNotMatch(repair, /catalog-search/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback states clean authority without exposing scoring history', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-history-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'the owner check still failed');
    const history = [
      { round: 1, beforeScore: 4, beforeMax: 6, afterScore: 4,
        afterMax: 6, result: 'kept with no score gain', remainingFailures: ['accounts/owner'] },
      { round: 2, beforeScore: 4, beforeMax: 6, afterScore: 4,
        afterMax: 6, result: 'kept with no score gain', remainingFailures: ['accounts/owner'] },
    ];
    const archive = join(app, 'stack-bench', 'records', 'bug-report-round2.md');
    const reported = spawnSync(process.execPath,
      [CLI, '--app', app, '--history-json', JSON.stringify(history), '--archive', archive],
      { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /clean database reset and a fresh/);
    assert.match(repair, /Earlier work/);
    assert.doesNotMatch(repair, /Round|4\/6|accounts\/owner|score/i);
    assert.match(repair, /existing local state/);
    assert.equal(readFileSync(archive, 'utf8'), repair);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
