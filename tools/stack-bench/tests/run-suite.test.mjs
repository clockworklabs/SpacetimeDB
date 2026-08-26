import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { attachRegressionScope, childFailureDetail, clearPreviousGradeOutputs, findMutationBackups, selectObservationScope,
  applicationFailureTotals, resetFailureOutcome, suitesForRecipe,
  databaseContainerForGrading, runGraderChild, verifyReseedProbe, waitForReseedProbe }
  from '../commands/run-suite.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.mjs';

const ECOMMERCE = join(import.meta.dirname, '..', 'tracks', 'ecommerce');

test('mutation backup scanning ignores volatile build caches and tolerates their removal', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-mutation-scan-'));
  try {
    mkdirSync(join(temp, 'server', 'src'), { recursive: true });
    mkdirSync(join(temp, 'client', 'node_modules', '.vite', 'deps_temp'), { recursive: true });
    const backup = join(temp, 'server', 'src', 'index.ts.mutation-backup');
    writeFileSync(backup, 'source');
    writeFileSync(join(temp, 'client', 'node_modules', '.vite', 'deps_temp',
      'cache.mutation-backup'), 'generated');
    assert.deepEqual(findMutationBackups(temp), [backup]);

    const disappearing = Object.assign(new Error('directory disappeared'), { code: 'ENOENT' });
    const directory = { name: 'source', isDirectory: () => true, isFile: () => false };
    assert.deepEqual(findMutationBackups('/app', {
      readDir: dir => {
        if (dir === '/app') return [directory];
        throw disappearing;
      },
    }), []);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('grader child diagnostics retain the cause instead of only trailing stack frames', () => {
  const stderr = [
    'Error: check evidence action is malformed',
    '    at validateCheckEvidence (check-evidence.mjs:1:1)',
    '    at buildCheckEvidence (grade.mjs:2:2)',
    '    at gradeFeature (grade.mjs:3:3)',
    '    at async main (grade.mjs:4:4)',
    '',
    'Node.js v24.18.1',
  ].join('\n');
  const detail = childFailureDetail({ stderr, message: 'command failed' });
  assert.match(detail, /^Error: check evidence action is malformed \|/);
  assert.match(detail, /gradeFeature/);
  assert.doesNotMatch(detail, /validateCheckEvidence/);
});

test('grader child diagnostics skip Node rejection boilerplate', () => {
  const stderr = [
    'node:internal/process/promises:394',
    '    triggerUncaughtException(err, true /* fromPromise */);',
    '    ^',
    '',
    'browserContext.close: Target page, context or browser has been closed',
    '    at closeAll (grade.mjs:596:21)',
    'Node.js v24.18.1',
  ].join('\n');
  assert.match(childFailureDetail({ stderr }),
    /^browserContext\.close: Target page, context or browser has been closed/);
});

test('grader subprocess output is retained with credentials redacted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grader-child-'));
  try {
    const result = runGraderChild(['grade.mjs'], root, 'account-create', {
      execute: () => ({ status: 1, signal: null,
        stdout: 'starting account-create\n',
        stderr: 'ANTHROPIC_API_KEY=should-not-leak\nError: browser closed\n' }),
    });
    assert(result.failure);
    assert.match(result.stderr, /\[redacted credential\]/);
    assert.doesNotMatch(result.stderr, /should-not-leak/);
    assert.equal(readFileSync(join(root, result.stdoutName), 'utf8'), 'starting account-create\n');
    assert.equal(readFileSync(join(root, result.stderrName), 'utf8'),
      '[redacted credential]\nError: browser closed\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('database grading uses the exact container from the authenticated run lease', () => {
  const calls = [];
  const container = databaseContainerForGrading('mongodb', {
    STACK_BENCH_LEASE: 'private/lease.json',
    STACK_BENCH_LEASE_TOKEN: 'secret-token',
  }, {
    readLease: (path, expected) => {
      calls.push({ path, expected });
      return { resources: { container: { name: 'stack-bench-mongodb' } } };
    },
  });
  assert.equal(container, 'stack-bench-mongodb');
  assert.deepEqual(calls, [{ path: 'private/lease.json',
    expected: { token: 'secret-token', backend: 'mongodb' } }]);
  assert.throws(() => databaseContainerForGrading('postgres', {
    STACK_BENCH_LEASE: 'private/lease.json',
  }), /both lease path and lease token/);
  assert.equal(databaseContainerForGrading('spacetime', {}), null);
});

test('generated layout and restart defects are repairable app failures, not harness failures', () => {
  const application = Object.assign(new Error('server/package.json has no dev or start script'),
    { code: 'generated_app_not_restartable' });
  assert.deepEqual(resetFailureOutcome(application),
    { kind: 'app_failure', phase: 'application-restart',
      appFailures: ['application-restart'] });
  assert.deepEqual(resetFailureOutcome(new Error('database container disappeared')),
    { kind: 'harness_failure', phase: 'database-reset' });
  assert.deepEqual(resetFailureOutcome({ status: GENERATED_APP_LAYOUT_EXIT_CODE }),
    { kind: 'app_failure', phase: 'application-layout',
      appFailures: ['application-layout'] });
});

test('reseed proof distinguishes a healthy empty app from restored startup data', async () => {
  const expectation = { jsonPath: 'items', minCount: 1 };
  const response = payload => ({ ok: true, status: 200, json: async () => payload });
  assert.deepEqual(await verifyReseedProbe('http://app/api/items', expectation,
    { fetchImpl: async () => response({ items: [{ id: 1 }] }) }),
  { ok: true, detail: null, count: 1 });
  assert.deepEqual(await verifyReseedProbe('http://app/api/items', expectation,
    { fetchImpl: async () => response({ items: [] }) }),
  { ok: false, count: 0,
    detail: 'startup data is missing: items contains 0 entries, expected at least 1' });
});

test('reset readiness still probes the application when no seed expectation is configured', async () => {
  let requests = 0;
  const result = await verifyReseedProbe('http://app/health', null, {
    fetchImpl: async () => {
      requests += 1;
      return { ok: true, status: 200 };
    },
  });
  assert.deepEqual(result, { ok: true, detail: null, count: null });
  assert.equal(requests, 1);
});

test('reset readiness rejects an unhealthy application without a seed expectation', async () => {
  const result = await verifyReseedProbe('http://app/health', null, {
    fetchImpl: async () => ({ ok: false, status: 503 }),
  });
  assert.deepEqual(result, {
    ok: false,
    count: null,
    detail: 'startup data probe returned HTTP 503',
  });
});

test('reseed readiness returns as soon as startup data is restored', async () => {
  const observed = [];
  const waits = [];
  const result = await waitForReseedProbe('http://app/api/items', { jsonPath: 'items', minCount: 1 }, {
    attempts: 5,
    intervalMs: 25,
    probe: async () => {
      observed.push(observed.length + 1);
      return observed.length < 3
        ? { ok: false, count: 0, detail: 'startup data is missing' }
        : { ok: true, count: 12, detail: null };
    },
    sleepImpl: async ms => waits.push(ms),
  });
  assert.deepEqual(result, { ok: true, count: 12, detail: null });
  assert.deepEqual(observed, [1, 2, 3]);
  assert.deepEqual(waits, [25, 25]);
});

test('reseed readiness returns the last failure after its bounded attempts', async () => {
  let calls = 0;
  const result = await waitForReseedProbe('http://app/api/items', {}, {
    attempts: 2, intervalMs: 0,
    probe: async () => ({ ok: false, count: calls++, detail: 'not ready' }),
    sleepImpl: async () => {},
  });
  assert.deepEqual(result, { ok: false, count: 1, detail: 'not ready' });
  assert.equal(calls, 2);
});

test('an application setup failure receives the exact current and inherited denominators', () => {
  const selection = { checks: [
    { executionId: 'current', points: 3 },
    { executionId: 'inherited', points: 2 },
    { executionId: 'control', points: 0 },
  ] };
  assert.deepEqual(applicationFailureTotals(selection, [
    { id: 'current' }, { id: 'inherited', inherited: true }, { id: 'control' },
  ]), { score: 0, max: 3, dirty: false, contractPass: null,
    regression: { score: 0, max: 2 } });
});

test('a new grade removes every prior grade output but keeps operator records', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grade-cleanup-'));
  try {
    mkdirSync(join(root, 'failure-media'), { recursive: true });
    writeFileSync(join(root, 'bundle.json'), 'old');
    writeFileSync(join(root, 'grading-features.json'), 'old');
    writeFileSync(join(root, 'grader-features.stdout.log'), 'old');
    writeFileSync(join(root, 'grader-features.stderr.log'), 'old');
    writeFileSync(join(root, 'grading-account-create@L1.json'), 'stale earlier level');
    writeFileSync(join(root, 'grader-account-create@L1.stdout.log'), 'stale earlier level');
    writeFileSync(join(root, 'operator-notes.txt'), 'keep');
    clearPreviousGradeOutputs(root);
    assert.equal(existsSync(join(root, 'bundle.json')), false);
    assert.equal(existsSync(join(root, 'grading-features.json')), false);
    assert.equal(existsSync(join(root, 'grader-features.stdout.log')), false);
    assert.equal(existsSync(join(root, 'grader-features.stderr.log')), false);
    assert.equal(existsSync(join(root, 'grading-account-create@L1.json')), false);
    assert.equal(existsSync(join(root, 'grader-account-create@L1.stdout.log')), false);
    assert.equal(existsSync(join(root, 'failure-media')), false);
    assert.equal(readFileSync(join(root, 'operator-notes.txt'), 'utf8'), 'keep');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('observed-only scope is modular, disjoint, and contributes no score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.5.0');
  const selected = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    observedSpecifications: ['ecommerce.spec.state-durability@1.1.0'],
  });
  const scored = selectObservationScope(selected, 'scored');
  const observed = selectObservationScope(selected, 'observed');
  assert.deepEqual(observed.checks, selected.selection.observedChecks);
  assert.equal(observed.scoredPoints, 0);
  assert(observed.observedPoints > 0);
  assert.equal(observed.checks.some(check => scored.checks.includes(check)), false);
  assert.throws(() => selectObservationScope(
    createBoundRecipeTaskRequest(binding, { featureIds: ['ecommerce.feature.accounts'] }),
    'observed'), /scope is empty/);
});

test('recipe-bound grading uses the recipe execution sources', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const suites = suitesForRecipe(track, binding);

  assert.match(suites.find(suite => suite.id === 'duplicate-checkout').spec,
    /01-duplicate-checkout-2\.3\.0\.json$/);
  assert.equal(suites.some(suite => suite.inherited), false);
});

test('hardened modular grading isolates the four direct server checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const suites = suitesForRecipe(track, binding);

  const expected = new Map([
    ['purchase-session', '101a'], ['purchase-attribution', '102a'],
    ['admin-write', '103a'], ['server-price', '104a'],
  ]);
  for (const [executionId, criterionId] of expected) {
    assert(suites.some(suite => suite.id === executionId));
    assert.equal(binding.release.checkCatalog.find(check => check.executionId === executionId)
      .criterionId, criterionId);
  }
});

test('recipe execution keeps inherited suites out of the current-level score', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.6.0');
  const suites = suitesForRecipe(track, binding);

  const inherited = suites.filter(suite => suite.inherited);
  assert.equal(inherited.length, 31);
  assert(inherited.every(suite => suite.fromLevel === 1));
  assert.equal(suites.filter(suite => !suite.inherited).length, 10);
});

test('L2 grading rechecks the exact selected L1 score without adding it to L2 points', () => {
  const track = loadTrack('ecommerce');
  const l1 = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const l2 = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.6.0');
  const prior = createBoundRecipeTaskRequest(l1, {
    featureIds: ['ecommerce.feature.accounts', 'ecommerce.feature.cart-checkout',
      'ecommerce.feature.catalog', 'ecommerce.feature.purchasing',
      'ecommerce.feature.reviews', 'ecommerce.feature.warehouse-admin'],
    expectedSpecifications: ['ecommerce.spec.access-control@1.2.0',
      'ecommerce.spec.concurrency-safety@1.3.0',
      'ecommerce.spec.external-data-sync@1.1.0', 'ecommerce.spec.live-state@1.2.0',
      'ecommerce.spec.state-durability@1.1.0',
      'ecommerce.spec.transactional-integrity@1.3.0'],
  });
  const current = createBoundRecipeTaskRequest(l2, {
    featureIds: ['ecommerce.inventory-operations-features',
      'ecommerce.operations-access-features', 'ecommerce.returns-pricing-features'],
    expectedSpecifications: ['ecommerce.inventory-operations-specifications@1.0.0',
      'ecommerce.operations-access-specifications@1.0.0',
      'ecommerce.returns-pricing-specifications@1.0.0'],
  });
  const scope = attachRegressionScope(current.selection, l2, suitesForRecipe(track, l2),
    prior.selection.scoredChecks.map(check => check.stableKey));
  assert.equal(scope.scoredPoints, 59);
  assert.equal(scope.regressionPoints, 58);
  assert.equal(scope.regressionChecks.length, prior.selection.scoredChecks.length);
  assert.equal(scope.checks.length,
    current.selection.scoredChecks.length + prior.selection.scoredChecks.length);
  assert.match(scope.evaluationSha256, /^[a-f0-9]{64}$/);
});

test('cumulative ownership survives inherited execution id renames', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-execution-ownership-'));
  const root = join(temp, 'ecommerce');
  try {
    cpSync(ECOMMERCE, root, { recursive: true });
    const recipe = join(root, 'composition', 'recipes', 'l2-standard-1.6.0.json');
    const value = JSON.parse(readFileSync(recipe, 'utf8'));
    for (const execution of value.execution) {
      if (execution.id.endsWith('@L1')) execution.id = `${execution.id.slice(0, -3)}-base`;
    }
    writeFileSync(recipe, `${JSON.stringify(value, null, 2)}\n`);
    const track = { ...loadTrack('ecommerce'), dir: root,
      suites: JSON.parse(readFileSync(join(root, 'track.json'), 'utf8')).suites };
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.6.0');
    const suites = suitesForRecipe(track, binding);
    const inherited = suites.filter(suite => suite.inherited);

    assert.equal(inherited.length, 31);
    assert(inherited.every(suite => suite.id.endsWith('-base')));
    assert(inherited.every(suite => suite.fromLevel === 1));
    assert.deepEqual(suites.filter(suite => !suite.inherited).map(suite => suite.id),
      ['features-existing@L2', 'low-stock@L2', 'invariants-existing@L2',
        'queue-warehouse@L2', 'transfer-totals@L2', 'paid-price-history@L2',
        'live-price@L2', 'self-contained@L2', 'strengthened@L2', 'server-actions@L2']);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});
