import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { writeArtifact } from '../src/evidence/artifacts.js';
import { parseReportBugsArgs } from '../commands/report-bugs.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { finding } from '../src/actions/action-findings.js';
import type { ActionEvidence } from '../src/actions/action-contract.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const CLI = join(STACK_BENCH_ROOT, 'dist', 'commands', 'report-bugs.js');

test('repair reports can read an isolated grading directory', () => {
  const args = parseReportBugsArgs(['node', 'report-bugs', '--app', '/app',
    '--results', '/results']);
  assert.equal(args.results, '/results');
  assert.equal(args.out, join('/app', 'BUG_REPORT.md'));
});

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
  statedBy?: string;
  desc?: string;
  evidence?: ReturnType<typeof createCheckEvidence>;
  setupEvidence?: ReturnType<typeof createCheckEvidence>;
}

function writeGrade(app: string, status: EvidenceStatus, summary: string,
  { grading = join(app, 'stack-bench'), feature = 'Accounts', points = 1,
    criterion = 'owner', stableKey = criterion, file = 'grading-features.json',
    url = 'http://app', consoleErrors = [], statedBy, desc, evidence: suppliedEvidence,
    setupEvidence: suppliedSetup }:
    WriteGradeOptions = {}): void {
  mkdirSync(grading, { recursive: true });
  const setupEvidence = suppliedSetup ?? (suppliedEvidence?.phase === 'setup' ? suppliedEvidence
    : createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
      startedAtMs: 1, completedAtMs: 2 }));
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
          desc: desc ?? statedBy ?? `Expected ${criterion}`, ...(statedBy ? { statedBy } : {}), points, evidence }] }],
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
    const repair = readFileSync(join(failedApp, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Actual:\*\* a failure was recorded without a detailed observation/);
    assert.doesNotMatch(repair, /INCONCLUSIVE: this wording/);
    for (const status of ['passed', 'inconclusive', 'harness_failure'] as const) {
      const app = join(root, `setup-${status}`);
      const copied = createCheckEvidence({ status: 'failed', code: 'test_result', phase: 'setup',
        startedAtMs: 1, completedAtMs: 2 });
      const setup = createCheckEvidence({ status, code: 'test_result', phase: 'setup',
        startedAtMs: 1, completedAtMs: 2 });
      writeGrade(app, 'failed', 'outer status cannot make setup repairable', { evidence: copied, setupEvidence: setup });
      const result = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
      assert.equal(result.status, 3, result.stderr);
      assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
    }
    const app = join(root, 'unmeasured-finding');
    writeGrade(app, 'failed', 'status cannot make an inconclusive finding repairable', {
      evidence: createCheckEvidence({ status: 'failed', code: 'test_result', phase: 'assertion',
        finding: finding('no-backend-control', { target: 'backend-runtime' }), startedAtMs: 1, completedAtMs: 2 }),
    });
    const result = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(result.status, 3, result.stderr);
    assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback includes actionable runtime evidence without private artifact names', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-diagnostics-'));
  try {
    const app = join(root, 'app');
    const total = finding('number-mismatch', { control: 'cart-total', observed: 9, expected: { equals: 12 } });
    const evidence = createCheckEvidence({
      status: 'failed', code: 'test_result', phase: 'assertion', actor: 'buyer',
      summary: 'cart total was wrong', observation: { value: 9 }, expected: { value: 12 },
      finding: total, startedAtMs: 3, completedAtMs: 4,
      actions: [{ actor: 'buyer', evidence: {
        schemaVersion: 2, action: { id: 'expectNumber', version: '1.0.0' },
        status: 'failed', type: 'browser-number', code: 'application_failure', phase: 'execute',
        summary: 'cart total was wrong', finding: total, observation: { value: 9 }, expected: { value: 12 },
        retryable: false, timing: { startedAtMs: 3, completedAtMs: 4, durationMs: 1,
          deadlineMs: 5_000 }, attachments: [], sensitivity: [],
      } }],
      attachments: [{ kind: 'screenshot', ref: 'failure-buyer.png' }],
    });
    writeGrade(app, 'failed', 'cart total was wrong', {
      feature: 'Cart', criterion: 'total', stableKey: 'private.check.cart.total',
      desc: 'the cart total equals the sum of its lines, with a 5 percent discount',
      statedBy: 'the cart survives a reload',
      url: 'http://app/cart', consoleErrors: ['POST /api/cart returned HTTP 500'], evidence,
    });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Actor\/session:\*\* buyer/);
    assert.match(repair, /Expected:\*\* the cart total equals the sum of its lines/);
    assert.match(repair, /Actual:\*\* the cart-total control reads 9, expected exactly 12/);
    assert.match(repair, /5 percent discount/);
    assert.doesNotMatch(repair, /the cart survives a reload/);
    assert.match(readFileSync(join(app, 'stack-bench', 'grading-features.json'), 'utf8'), /"equals": 12/);
    assert.doesNotMatch(repair, /cart total was wrong/);
    assert.doesNotMatch(repair, /Application URL|http:\/\//);
    assert.doesNotMatch(repair, /failure-buyer\.png/);
    assert.match(repair, /Console or network errors:[\s\S]*HTTP 500/);
    assert.doesNotMatch(repair, /expect number|private\.check|Stack Bench|grader|criterion/i);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback describes behavior instead of browser commands', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-browser-language-'));
  try {
    const app = join(root, 'app');
    const detail = `locator.selectOption: Timeout 5000ms exceeded.\n`
      + `waiting for locator('[data-testid="notification-frequency"]')`;
    const choice = finding('choice-missing', { control: 'notification-frequency', detail });
    const evidence = createCheckEvidence({
      status: 'failed', code: 'test_result', phase: 'assertion', summary: detail,
      finding: choice, observation: detail, startedAtMs: 3, completedAtMs: 4,
      actions: [{ actor: 'owner', evidence: {
        schemaVersion: 2, action: { id: 'fill', version: '1.0.0' },
        status: 'failed', type: 'browser-interaction-evidence', code: 'application_failure',
        phase: 'execute', summary: detail, finding: choice, observation: null, expected: null,
        retryable: false, timing: { startedAtMs: 3, completedAtMs: 4,
          durationMs: 1, deadlineMs: 60_000 }, attachments: [], sensitivity: [],
      } }],
    });
    writeGrade(app, 'failed', detail, { evidence });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Actual:\*\* the notification-frequency control did not offer the required choice/);
    assert.match(repair, /Failed action:\*\* Select the requested choice/);
    assert.doesNotMatch(repair, /locator|selectOption|data-(?:role|testid)|Timeout|5000ms|http:\/\//);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('failed reload reports its transport error without claiming later checks ran or leaking diagnostics', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-reload-'));
  try {
    for (const [code, expected] of [['ERR_CONNECTION_REFUSED', 'the browser request failed (ERR_CONNECTION_REFUSED)'],
      ['ERR_PRIVATE_DIAGNOSTIC', 'the page did not behave as required']]) {
      const app = join(root, code!);
      const detail = `page.reload: net::${code} at http://user:password@app/private-probe?token=secret\nPRIVATE_STACK`;
      const failure = finding('page-error', { detail });
      const evidence = createCheckEvidence({ status: 'failed', code: 'test_result', phase: 'assertion',
        actor: 'owner', summary: detail, finding: failure, startedAtMs: 1, completedAtMs: 2,
        actions: [{ actor: 'owner', evidence: {
          schemaVersion: 2, action: { id: 'reload', version: '1.0.0' }, status: 'failed',
          type: 'browser-interaction-evidence', code: 'application_failure', phase: 'execute',
          summary: detail, finding: failure, observation: null, expected: null, retryable: false,
          timing: { startedAtMs: 1, completedAtMs: 2, durationMs: 1, deadlineMs: 5000 },
          attachments: [], sensitivity: [],
        } }],
      });
      writeGrade(app, 'failed', detail, { evidence, desc: 'saved history survives reload and a fresh login' });
      const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
      assert.equal(reported.status, 0, reported.stderr);
      const report = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
      assert(report.includes(`**Actual:** ${expected}`));
      assert.match(report, /Failed action:\*\* Reload the page/);
      assert.match(report, /sequence stopped at this action; later behavior was not observed/);
      assert.doesNotMatch(report, /Completed lifecycle actions|page reloaded|PRIVATE_|private-probe|password|secret|http:\/\//);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('repair feedback refuses internal evaluation language', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-disclosure-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'x', { statedBy: 'the Stack Bench test failed' });
    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 2);
    assert.match(reported.stderr, /contains internal language/);
    assert.equal(existsSync(join(app, 'BUG_REPORT.md')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair context identifies early control and value failures without claiming later durability was observed', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-context-'));
  try {
    for (const missing of [finding('control-missing', { control: 'cart-item', filtered: true }),
      finding('value-mismatch', { control: 'staff-role-select', observed: 'staff', expected: 'inventory' }),
      finding('number-mismatch', { control: 'item-stock', observed: 100, expected: { equals: 99 } })]) {
    const action = (id: string, observation: unknown, failed = false): { actor: string; evidence: ActionEvidence } => ({
      actor: 'shopper', evidence: {
        schemaVersion: 2, action: { id, version: '1.0.0' }, status: failed ? 'failed' : 'passed',
        type: 'test-evidence', code: failed ? 'application_failure' : 'completed', phase: 'execute',
        summary: 'PRIVATE_DIAGNOSTIC', finding: failed ? missing : null,
        observation, expected: { internal: 'PRIVATE_EXPECTATION' }, retryable: false,
        timing: { startedAtMs: 1, completedAtMs: 2, durationMs: 1, deadlineMs: 100 },
        attachments: [], sensitivity: [],
      },
    });
    for (const restarted of [false, true]) {
      const app = join(root, restarted ? 'after' : 'before');
      const actions = ['add-to-cart', 'cart-toggle', 'checkout-submit', 'catalog-link', 'add-to-cart', 'cart-toggle']
        .map(control => action('click', { clicked: control }));
      actions.unshift(action('callAction', { action: 'submitReview', accepted: true, status: 201,
        body: 'PRIVATE_BODY', url: 'http://PRIVATE_URL' }));
      actions.push(action('click', { clicked: false, testid: 'SKIPPED_CONTROL' }),
        action('fill', { value: 'PRIVATE_PASSWORD' }), action('runScript', { script: 'PRIVATE_SCRIPT' }));
      if (restarted) actions.push(action('reload', { reloaded: true }),
        action('stopAppServer', { operation: 'stop' }), action('startAppServer', { operation: 'start' }),
        action('restartBackend', { operation: 'restart' }));
      actions.push(action('expect', null, true));
      // A later record must not be reported as a completed action before the failure.
      actions.push(action('restartBackend', { operation: 'restart' }));
      actions.push(action('callAction', { action: 'FUTURE_STRANGER_REQUEST', accepted: false, status: 403 }));
      const evidence = createCheckEvidence({ status: 'failed', code: 'application_failure',
        phase: 'assertion', actor: 'shopper', finding: missing, actions,
        startedAtMs: 1, completedAtMs: 2 });
      writeGrade(app, 'failed', 'missing item', { evidence, feature: 'Checkout',
        desc: 'cart and order history survive a page reload and runtime restart' });
      const result = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      const report = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
      assert.match(report, /Expected:\*\* cart and order history survive a page reload and runtime restart/);
      assert.match(report, /shopper: checkout-submit → shopper: catalog-link → shopper: add-to-cart → shopper: cart-toggle/);
      assert.match(report, /shopper: submitReview returned HTTP 201/);
      assert.match(report, /The sequence stopped at this (control|value check); later behavior was not observed/);
      if (restarted) assert.match(report, /Completed lifecycle actions: page reloaded; application server stopped; application server started; database runtime restarted/);
      else assert.doesNotMatch(report, /Completed lifecycle actions/);
      assert.doesNotMatch(report, /PRIVATE_|FUTURE_STRANGER_REQUEST|SKIPPED_CONTROL|lost|use a transaction|implement|retry policy/i);
    }
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('setup feedback reports the failed control without claiming the later guarantee failed', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-setup-'));
  try {
    const evidence = createCheckEvidence({ status: 'failed', code: 'application_failure',
      phase: 'setup', startedAtMs: 1, completedAtMs: 2,
      finding: finding('control-missing', { control: 'item-stock', filtered: false }) });
    writeGrade(root, 'failed', 'setup failed', { evidence,
      statedBy: 'the server refuses an unauthenticated purchase' });
    const reported = spawnSync(process.execPath, [CLI, '--app', root], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const report = readFileSync(join(root, 'BUG_REPORT.md'), 'utf8');
    assert.match(report, /item-stock control did not appear/);
    assert.match(report, /Setup stopped before the named behavior was reached/);
    assert.doesNotMatch(report, /Expected:|unauthenticated purchase/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('copied setup failures use original observations once and retain distinct setup failures', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-setup-dedup-'));
  try {
    const copied = createCheckEvidence({ status: 'failed', code: 'application_failure',
      phase: 'setup', startedAtMs: 1, completedAtMs: 2 });
    const setup = createCheckEvidence({ status: 'failed', code: 'application_failure',
      phase: 'setup', actor: 'buyer', startedAtMs: 1, completedAtMs: 2,
      finding: finding('number-mismatch', { control: 'item-stock', observed: 100,
        expected: { equals: 99 }, scopeText: 'Bluetooth Speaker' }) });
    for (const criterion of ['authentication', 'ownership']) {
      writeGrade(root, 'failed', 'later behavior did not run', { criterion,
        file: `grading-${criterion}.json`, evidence: copied, setupEvidence: setup,
        statedBy: 'unauthorized purchases must be refused',
        consoleErrors: ['POST /api/buy returned 500', 'POST /api/buy returned 500'] });
    }
    writeGrade(root, 'failed', 'different setup failure', { criterion: 'other',
      file: 'grading-other.json', evidence: copied,
      setupEvidence: createCheckEvidence({ ...setup,
        startedAtMs: 3, completedAtMs: 4,
        finding: finding('control-missing', { control: 'orders-toggle', filtered: false }) }),
      statedBy: 'orders must survive a restart' });
    const reported = spawnSync(process.execPath, [CLI, '--app', root], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const report = readFileSync(join(root, 'BUG_REPORT.md'), 'utf8');
    assert.equal((report.match(/### Bug /g) ?? []).length, 2);
    assert.equal((report.match(/item-stock control reads 100, expected exactly 99/g) ?? []).length, 1);
    assert.match(report, /orders-toggle control did not appear/);
    assert.match(report, /entry matching "Bluetooth Speaker"/);
    assert.equal((report.match(/Setup stopped before the named behavior was reached/g) ?? []).length, 2);
    assert.equal((report.match(/POST \/api\/buy returned 500/g) ?? []).length, 1);
    assert.doesNotMatch(report, /unauthorized purchases|orders must survive|Expected:\*\*/);
  } finally { rmSync(root, { recursive: true, force: true }); }
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
    assert.match(repair, /Preserve earlier fixes/);
    assert.doesNotMatch(repair, /Earlier changes did not fix/);
    assert.doesNotMatch(repair, /remaining:/);
    assert.doesNotMatch(repair, /Catalog|catalog search failed/);
    assert.doesNotMatch(repair, /check\.catalog\.search/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback includes failures caused by the rejected prior repair', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-regression-'));
  try {
    const app = join(root, 'app');
    writeGrade(app, 'failed', 'checkout still fails', {
      feature: 'Checkout', criterion: 'checkout', stableKey: 'check.checkout',
    });
    const regression = join(root, 'regression.md');
    writeFileSync(regression, [
      '## Behavior', '',
      '### Bug 1: Accounts', '', '**Expected:** the owner keeps access', '',
      '**Actual:** the owner was signed out', '',
    ].join('\n'));

    const reported = spawnSync(process.execPath, [CLI, '--app', app,
      '--prior-regression', regression], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.match(repair, /Actual:\*\* a failure was recorded without a detailed observation/);
    assert.match(repair, /Previous repair regression/);
    assert.match(repair, /Accounts/);
    assert.match(repair, /owner was signed out/);
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
    assert.match(repair, /Expected:\*\* Expected owner/);
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

test('dependency repair feedback contains only interfaces selected for that feature', () => {
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
    assert.match(repair, /Preserve earlier fixes/);
    assert.doesNotMatch(repair, /Earlier changes did not fix/);
    assert.doesNotMatch(repair, /Round|4\/6|accounts\/owner|score/i);
    assert.match(repair, /existing local[\s\n]+state/);
    assert.equal(readFileSync(archive, 'utf8'), repair);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair feedback uses behavioral expectations and findings without implementation advice', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-sources-'));
  try {
    const app = join(root, 'app');
    const delivered = finding('message-delivered', { actor: 'other' });
    const evidence = createCheckEvidence({
      status: 'failed', code: 'test_result', phase: 'assertion', actor: 'other',
      summary: '"support-secret-619" was delivered to other', finding: delivered,
      observation: 'other received support-secret-619 in the ticket list',
      expected: 'no message containing support-secret-619 reaches other',
      startedAtMs: 3, completedAtMs: 4,
      actions: [{ actor: 'other', evidence: {
        schemaVersion: 2, action: { id: 'expectNotReceived', version: '1.0.0' },
        status: 'failed', type: 'transport-evidence', code: 'application_failure', phase: 'execute',
        summary: '"support-secret-619" was delivered to other', finding: delivered,
        observation: 'other received support-secret-619', expected: null,
        retryable: false, timing: { startedAtMs: 3, completedAtMs: 4, durationMs: 1,
          deadlineMs: 5_000 }, attachments: [], sensitivity: [],
      } }],
    });
    writeGrade(app, 'failed', 'unused', { feature: 'Customer support history', criterion: '612b',
      statedBy: 'a support message is visible only to its customer and to staff',
      consoleErrors: ['POST /api/support returned 500'], evidence });

    const reported = spawnSync(process.execPath, [CLI, '--app', app], { encoding: 'utf8' });
    assert.equal(reported.status, 0, reported.stderr);
    const repair = readFileSync(join(app, 'BUG_REPORT.md'), 'utf8');
    assert.doesNotMatch(repair, /support-secret-619|ticket list|was delivered to other"/);
    assert.match(repair, /Expected:\*\* a support message is visible only to its customer and to staff/);
    assert.match(repair, /Actual:\*\* private data was delivered to unauthorized actor other/);
    assert.match(repair, /Console or network errors:[\s\S]*POST \/api\/support returned 500/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
