import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBoundRecipeTaskRequest, selectScenarioChecks } from '../src/composition/recipe-selection.js';
import { isModularRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { attachRegressionScope, childFailureDetail, clearPreviousGradeOutputs, findMutationBackups, selectObservationScope,
  applicationFailureTotals, codeMetrics, resetFailureOutcome, suitesForRecipe,
  checkRuntimeDatabaseProvenance, databaseProvenanceFailure,
  contractLintArgv, databaseLeaseForGrading, databaseNameForGrading, runGraderChild,
  verifyApplicationProbe, waitForApplicationProbe, closeSuiteBrowser, preserveStartFailure, suiteMayRetry }
  from '../commands/run-suite.js';
import { loadTrack } from '../src/composition/tracks.js';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { readArtifactPayload } from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';

// Recovery must not erase product failures, ignore cleanup, or retry unknown errors.
test('suite recovery requires exclusively passed or explicitly retryable inconclusive evidence', () => {
  const criterion = (status: 'passed' | 'failed' | 'blocked' | 'inconclusive' | 'harness_failure', retryable = false) => ({
    id: status, evidence: createCheckEvidence({ status, retryable, code: 'test_result',
      phase: status === 'blocked' ? 'setup' : 'assertion', startedAtMs: 0, completedAtMs: 1 }),
  });
  const grade = (...criteria: ReturnType<typeof criterion>[]) => ({ total: 0, max: 1,
    features: [{ name: 'test', criteria }] });
  assert.equal(suiteMayRetry(grade(criterion('inconclusive', true))), true);
  assert.equal(suiteMayRetry(grade(criterion('passed'), criterion('inconclusive', true))), true);
  for (const status of ['failed', 'blocked', 'harness_failure', 'inconclusive'] as const) {
    assert.equal(suiteMayRetry(grade(criterion(status), criterion('inconclusive', true))), false, status);
  }
  assert.equal(suiteMayRetry(grade()), false);
  assert.equal(suiteMayRetry(grade(criterion('passed'))), false);
  const cleanup = grade(criterion('inconclusive', true));
  assert.equal(suiteMayRetry({ ...cleanup, features: [{ ...cleanup.features[0]!,
    cleanupEvidence: { status: 'harness_failure', failures: [{ stage: 'context-close' }] } }] }), false);
  assert.equal(suiteMayRetry({ total: 0, max: 1, features: [{ name: 'missing', criteria: [{ id: 'missing' }] }] }), false);
});

const ECOMMERCE = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');

test('application startup failures retain useful logs with credentials removed', () => {
  const output = mkdtempSync(join(tmpdir(), 'startup-failure-log-'));
  try {
    preserveStartFailure({ startLog: 'early cause\npassword=do-not-publish\nlast line' }, output);
    const log = readFileSync(join(output, 'application-start.log'), 'utf8');
    assert.match(log, /early cause/);
    assert.match(log, /last line/);
    assert.doesNotMatch(log, /do-not-publish/);
  } finally { rmSync(output, { recursive: true, force: true }); }
});

test('browser shutdown failure replaces a previously written app outcome before returning', async () => {
  const bundle = { error: 'app restart failed',
    outcome: { kind: 'app_failure', phase: 'application-start', reason: 'app restart failed' } };
  let saved = structuredClone(bundle);
  const failure = new Error('browser control disconnected');
  await assert.rejects(closeSuiteBrowser({ close: async () => { throw failure; } }, bundle,
    () => { saved = structuredClone(bundle); }), error => error === failure);
  assert.equal(saved.outcome.kind, 'harness_failure');
  assert.equal(saved.outcome.phase, 'grading-cleanup');
  assert.match(saved.outcome.reason, /browser control disconnected/);
  await closeSuiteBrowser({ close: async () => {} }, bundle,
    () => assert.fail('successful cleanup must not replace evidence'));
});

function sequentialL2Track() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-l2-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  return { temp, track: { ...loadTrack('ecommerce'), dir: root } };
}

test('a targeted staff repair writes explicit empty lint evidence without falling back to level contracts', () => {
  const out = mkdtempSync(join(tmpdir(), 'stack-bench-empty-lint-'));
  try {
    for (const level of [2, 3]) {
      const binding = resolveRecipeRelease(loadTrack('ecommerce'), level, 'ecommerce.progression-catalog');
      const selected = createBoundRecipeTaskRequest(binding, {
        featureIds: ['ecommerce.progression.staff-access'],
        taskMode: 'upgrade',
      });
      assert(selected.selection.checks.length > 0, 'scenario grading must still select staff checks');
      const argv = contractLintArgv({
        url: 'http://127.0.0.1:1', level: String(level), track: 'ecommerce', label: 'staff-repair',
        out, bundleArtifactId: 'staff-repair',
      }, selected);
      assert(argv.includes('--selected-hooks'));
      assert.equal(argv.includes('--hook'), false);
      const result = spawnSync(process.execPath, argv, { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr || result.stdout);
      const report = readArtifactPayload<{ selectedHooks: string[]; pass: boolean; results: unknown[] }>(
        join(out, 'contract-lint.json'), { expectedKind: 'contract_lint' });
      assert.deepEqual(report.selectedHooks, []);
      assert.equal(report.pass, true);
      assert.deepEqual(report.results, []);

      const unscoped = spawnSync(process.execPath, argv.filter(value => value !== '--selected-hooks'),
        { encoding: 'utf8' });
      assert.equal(unscoped.status, 2);
      assert.match(unscoped.stderr, /No contracts found/);
    }
  } finally { rmSync(out, { recursive: true, force: true }); }
});

test('code metrics count each package manifest once when server code is at the app root', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-code-metrics-'));
  try {
    mkdirSync(join(temp, 'client'), { recursive: true });
    writeFileSync(join(temp, 'index.js'), 'export const app = true;\n');
    writeFileSync(join(temp, 'package.json'), JSON.stringify({
      dependencies: { express: '1', mongodb: '1' },
    }));
    writeFileSync(join(temp, 'client', 'package.json'), JSON.stringify({
      dependencies: { react: '1' },
    }));

    assert.equal(codeMetrics({ app: temp, backend: 'mongodb' }).runtimeDeps, 3);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

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

  const rejection = [
    'node:internal/process/promises:394',
    '    triggerUncaughtException(err, true /* fromPromise */);',
    '    ^',
    '',
    'browserContext.close: Target page, context or browser has been closed',
    '    at closeAll (grade.mjs:596:21)',
    'Node.js v24.18.1',
  ].join('\n');
  assert.match(childFailureDetail({ stderr: rejection }),
    /^browserContext\.close: Target page, context or browser has been closed/);

  const processDetail = childFailureDetail({
    stderr: 'hosted application port 6301 still has a listener',
    message: 'Command failed: docker exec generated-app sh -lc <large command>',
  });
  assert.equal(processDetail, 'hosted application port 6301 still has a listener');
});

test('grader subprocesses run asynchronously and retain redacted output', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grader-child-'));
  try {
    const result = await runGraderChild(['--eval',
      "console.log('starting account-create'); console.error('ANTHROPIC_API_KEY=should-not-leak'); process.exit(1)"],
    root, 'account-create');
    assert(result.failure);
    assert.match(result.stderr, /\[redacted credential\]/);
    assert.doesNotMatch(result.stderr, /should-not-leak/);
    assert.equal(readFileSync(join(root, result.stdoutName), 'utf8'), 'starting account-create\n');
    assert.equal(readFileSync(join(root, result.stderrName), 'utf8'), '[redacted credential]\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('database grading uses the exact container from the authenticated run lease', () => {
  const calls: Array<{ path: string;
    expected?: { token?: string; backend?: string; active?: boolean } }> = [];
  const lease = databaseLeaseForGrading('mongodb', {
    STACK_BENCH_LEASE: 'private/lease.json',
    STACK_BENCH_LEASE_TOKEN: 'secret-token',
  }, {
    readLease: (path, expected) => {
      calls.push({ path, expected });
      return createBackendLease({ runId: 'grading-test', backend: 'mongodb', track: 'ecommerce',
        runIndex: 0, database: 'app_ecommerce_run0', container: {
          name: 'stack-bench-mongodb', id: 'mongodb-container-id' } });
    },
  });
  assert.equal(lease?.resources.container?.name, 'stack-bench-mongodb');
  assert.equal(lease?.resources.container?.id, 'mongodb-container-id');
  assert.deepEqual(calls, [{ path: 'private/lease.json',
    expected: { token: 'secret-token', backend: 'mongodb', active: true } }]);
  assert.throws(() => databaseLeaseForGrading('postgres', {
    STACK_BENCH_LEASE: 'private/lease.json',
  }), /both lease path and lease token/);
  assert.equal(databaseLeaseForGrading('spacetime', {}), null);

  const env = { STACK_BENCH_LEASE: 'private/lease.json', STACK_BENCH_LEASE_TOKEN: 'secret-token' };
  const convex = databaseLeaseForGrading('convex', env, { readLease: (_path, expected) => {
    assert.deepEqual(expected, { token: 'secret-token', backend: 'convex', active: true });
    const native = createBackendLease({ runId: 'grading-convex', backend: 'convex', track: 'ecommerce', runIndex: 0,
      serverUri: 'http://127.0.0.1:14310' });
    native.resources.container = { name: 'owned-convex', id: 'a'.repeat(64), owned: true };
    return native;
  } });
  assert.equal(convex?.resources.serverUri, 'http://127.0.0.1:14310');
  assert.equal(convex?.resources.container?.id, 'a'.repeat(64));

  const spacetimeLease = () => createBackendLease({ runId: 'grading-test', backend: 'spacetime',
    track: 'ecommerce', runIndex: 0, module: 'app_ecommerce_run0',
    serverUri: 'http://127.0.0.1:3210', dataDir: join(tmpdir(), 'stack-bench-spacetime-test') });
  const spacetime = databaseLeaseForGrading('spacetime', env, { readLease: spacetimeLease });
  assert.equal(spacetime?.resources.module, 'app_ecommerce_run0');
  assert.equal(spacetime?.resources.serverUri, 'http://127.0.0.1:3210');
  assert.throws(() => databaseLeaseForGrading('spacetime', env, { readLease: () => {
    const incomplete = spacetimeLease();
    incomplete.resources.module = null;
    return incomplete;
  } }), /no complete module target/);
});

test('database grading uses the exact database from the authenticated run lease', () => {
  const track = { slug: 'ecom' };
  assert.equal(databaseNameForGrading(track, 3), 'app_ecom_run3');
  assert.equal(databaseNameForGrading(track, 3, {
    resources: { database: 'leased_database' },
  }), 'leased_database');
  assert.throws(() => databaseNameForGrading(track, 3, { resources: {} }),
    /active database lease has no database name/);
});

test('runtime database proof requires an authenticated lease', () => {
  assert.deepEqual(checkRuntimeDatabaseProvenance({ backend: 'spacetime' }), {
    ok: null,
    verified: false,
    reason: 'standalone grading has no authenticated database lease',
  });
  assert.deepEqual(databaseProvenanceFailure(new Error('docker command failed')), {
    kind: 'harness_failure',
    phase: 'database-provenance',
    reason: 'runtime database provenance failed: docker command failed',
  });
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

test('application readiness returns as soon as the public app responds', async () => {
  const observed: number[] = [];
  const waits: number[] = [];
  const result = await waitForApplicationProbe('http://app', {
    attempts: 5,
    intervalMs: 25,
    probe: async () => {
      observed.push(observed.length + 1);
      return observed.length < 3
        ? { ok: false, detail: 'not ready' }
        : { ok: true, detail: null };
    },
    sleepImpl: async ms => { waits.push(ms); },
  });
  assert.deepEqual(result, { ok: true, detail: null });
  assert.deepEqual(observed, [1, 2, 3]);
  assert.deepEqual(waits, [25, 25]);

  let calls = 0;
  const exhausted = await waitForApplicationProbe('http://app', {
    attempts: 2, intervalMs: 0,
    probe: async () => { calls += 1; return { ok: false, detail: 'not ready' }; },
    sleepImpl: async () => {},
  });
  assert.deepEqual(exhausted, { ok: false, detail: 'not ready' });
  assert.equal(calls, 2);
});

test('an application setup failure receives the exact current and inherited denominators', () => {
  const selection = { checks: [
    { executionId: 'current', points: 3 },
    { executionId: 'inherited', points: 2 },
    { executionId: 'control', points: 0 },
  ] };
  const suites = [{ id: 'current' }, { id: 'inherited', inherited: true }, { id: 'control' }];
  assert.deepEqual(applicationFailureTotals(selection, suites, new Set()), { score: 0, max: 3, dirty: false,
    contractPass: null, regression: { score: 0, max: 2 } });
  const measured = { checks: selection.checks.map((check, index) => ({ ...check, stableKey: `check-${index}` })) };
  assert.equal(applicationFailureTotals(measured, suites, new Set(['check-0', 'check-1'])).score, 3,
    'passes measured before the abort count; inherited regression guards stay unscored');
});

test('a new grade removes every prior grade output but keeps operator records', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grade-cleanup-'));
  try {
    mkdirSync(join(root, 'failure-media'), { recursive: true });
    mkdirSync(join(root, 'database-provenance'), { recursive: true });
    writeFileSync(join(root, 'bundle.json'), 'old');
    writeFileSync(join(root, 'grading-features.json'), 'old');
    writeFileSync(join(root, 'grader-features.stdout.log'), 'old');
    writeFileSync(join(root, 'grader-features.stderr.log'), 'old');
    writeFileSync(join(root, 'application-start.log'), 'old startup failure');
    writeFileSync(join(root, 'grading-account-create@L1.json'), 'stale earlier level');
    writeFileSync(join(root, 'grader-account-create@L1.stdout.log'), 'stale earlier level');
    writeFileSync(join(root, 'operator-notes.txt'), 'keep');
    clearPreviousGradeOutputs(root);
    assert.equal(existsSync(join(root, 'bundle.json')), false);
    assert.equal(existsSync(join(root, 'grading-features.json')), false);
    assert.equal(existsSync(join(root, 'grader-features.stdout.log')), false);
    assert.equal(existsSync(join(root, 'grader-features.stderr.log')), false);
    assert.equal(existsSync(join(root, 'application-start.log')), false);
    assert.equal(existsSync(join(root, 'grading-account-create@L1.json')), false);
    assert.equal(existsSync(join(root, 'grader-account-create@L1.stdout.log')), false);
    assert.equal(existsSync(join(root, 'failure-media')), false);
    assert.equal(existsSync(join(root, 'database-provenance')), false);
    assert.equal(readFileSync(join(root, 'operator-notes.txt'), 'utf8'), 'keep');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('observed-only scope is modular, disjoint, and contributes no score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.sequential-l1');
  const selected = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    observedSpecifications: ['ecommerce.spec.state-durability'],
  });
  assert(isModularRecipeTaskRequest(selected));
  const scored = selectObservationScope(selected, 'scored');
  const observed = selectObservationScope(selected, 'observed');
  assert(scored);
  assert(observed);
  assert(observed.observedPoints !== undefined);
  assert.deepEqual(observed.checks, selected.selection.observedChecks);
  assert.equal(observed.scoredPoints, 0);
  assert(observed.observedPoints > 0);
  let measurementPoints = 0;
  for (const source of new Set(observed.checks.map(check => check.source))) {
    assert(source);
    const checks = observed.checks.filter(check => check.source === source);
    const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(ECOMMERCE, source), 'utf8')));
    const measured = selectScenarioChecks(scenario, { checks }, checks.map(check => check.stableKey));
    measurementPoints += measured.features.flatMap(feature => feature.criteria)
      .reduce((sum, criterion) => sum + criterion.points, 0);
  }
  assert.equal(measurementPoints, observed.observedPoints);
  assert.equal(observed.checks.some(check => scored.checks.includes(check)), false);
  assert.throws(() => selectObservationScope(
    createBoundRecipeTaskRequest(binding, { featureIds: ['ecommerce.feature.accounts'] }),
    'observed'), /scope is empty/);
});

test('recipe weights govern scenario grading with and without explicit check filters', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1, 'ecommerce.sequential-l1');
  let total = 0;
  let overridden = 0;
  let zeroPoint = 0;
  for (const execution of binding.plan.execution) {
    const checks = binding.release.checkCatalog.filter(check => check.executionId === execution.id);
    const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(ECOMMERCE, execution.source), 'utf8')));
    const original = structuredClone(scenario);
    for (const keys of [[], checks.map(check => check.stableKey)]) {
      const selected = selectScenarioChecks(scenario, { checks }, keys);
      for (const check of checks) {
        const criterion = selected.features.find(feature => feature.id === check.featureId)?.criteria
          .find(candidate => candidate.id === check.criterionId);
        assert(criterion);
        assert.equal(criterion.points, check.points, check.stableKey);
      }
      assert.deepEqual(scenario, original, 'grading weights must not change the source definition');
    }
    assert.equal(selectScenarioChecks(scenario, null).features, scenario.features);
    for (const check of checks) {
      total += check.points;
      const source = scenario.features.find(feature => feature.id === check.featureId)?.criteria
        .find(criterion => criterion.id === check.criterionId);
      assert(source);
      if (check.points !== source.points) overridden += 1;
      if (check.points === 0) zeroPoint += 1;
    }
  }
  assert.equal(total, 57);
  assert(overridden > 0, 'the regression must exercise a recipe weight override');
  assert(zeroPoint > 0, 'zero-point controls must retain zero weight');
});

test('recipe execution keeps inherited suites out of the current-level score', () => {
  const { temp, track } = sequentialL2Track();
  try {
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2');
    const suites = suitesForRecipe(track, binding);
    const inherited = suites.filter(suite => suite.inherited);
    assert.equal(inherited.length, 37);
    assert(inherited.every(suite => suite.fromLevel === 1));
    assert.equal(suites.filter(suite => !suite.inherited).length, 11);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('L2 grading rechecks the exact selected L1 score without adding it to L2 points', () => {
  const { temp, track } = sequentialL2Track();
  try {
    const l1 = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1');
    const l2 = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2');
    const prior = createBoundRecipeTaskRequest(l1, {
      featureIds: ['ecommerce.feature.accounts', 'ecommerce.feature.cart-checkout',
        'ecommerce.feature.catalog', 'ecommerce.feature.purchasing',
        'ecommerce.feature.reviews', 'ecommerce.feature.warehouse-admin'],
      expectedSpecifications: ['ecommerce.spec.access-control',
        'ecommerce.spec.concurrency-safety',
        'ecommerce.spec.external-data-sync', 'ecommerce.spec.live-state',
        'ecommerce.spec.state-durability',
        'ecommerce.spec.transactional-integrity'],
    });
    const current = createBoundRecipeTaskRequest(l2, {
      featureIds: ['ecommerce.inventory-operations-features',
        'ecommerce.operations-access-features', 'ecommerce.returns-pricing-features'],
      expectedSpecifications: ['ecommerce.inventory-operations-specifications',
        'ecommerce.operations-access-specifications',
        'ecommerce.returns-pricing-specifications'],
      dependencyExpansion: 'exact',
    });
    assert(isModularRecipeTaskRequest(prior));
    assert(isModularRecipeTaskRequest(current));
    const scope = attachRegressionScope(current.selection, l2, suitesForRecipe(track, l2),
      prior.selection.scoredChecks.map(check => check.stableKey));
    assert(scope);
    assert(scope.regressionChecks);
    assert(scope.evaluationSha256);
    assert.equal(scope.scoredPoints, current.selection.scoredPoints);
    assert.equal(scope.regressionPoints, prior.selection.scoredPoints);
    assert.equal(scope.regressionChecks.length, prior.selection.scoredChecks.length);
    assert.equal(scope.checks.length,
      current.selection.scoredChecks.length + prior.selection.scoredChecks.length);
    assert.match(scope.evaluationSha256, /^[a-f0-9]{64}$/);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('a readiness probe timeout is recorded as a timeout, not a refusal', async () => {
  let requests = 0;
  assert.deepEqual(await verifyApplicationProbe('http://app', {
    fetchImpl: async () => { requests += 1; return { ok: true, status: 200 }; },
  }), { ok: true, detail: null });
  assert.equal(requests, 1);
  assert.deepEqual(await verifyApplicationProbe('http://app', {
    fetchImpl: async () => ({ ok: false, status: 503 }),
  }), { ok: false, detail: 'application returned HTTP 503' });
  const probe = (error: Error) => verifyApplicationProbe('http://app', { fetchImpl: async () => { throw error; } });
  const timedOut = await probe(Object.assign(new Error('The operation was aborted due to timeout'), { name: 'TimeoutError' }));
  assert.equal(timedOut.ok, false);
  assert.equal(timedOut.timedOut, true);
  const refused = await probe(new TypeError('fetch failed'));
  assert.equal(refused.ok, false);
  assert.equal(refused.timedOut, undefined);
});

test('code metrics read only regular files the app owns', t => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-code-metrics-link-'));
  try {
    const app = join(temp, 'app');
    mkdirSync(app);
    writeFileSync(join(app, 'index.js'), 'export const app = true;\n');
    writeFileSync(join(temp, 'outside.txt'), 'a\nb\nc\nd\ne\nf\n');
    try { symlinkSync(join(temp, 'outside.txt'), join(app, 'planted.js')); }
    catch { t.skip('this host cannot create file symlinks'); return; }
    const metrics = codeMetrics({ app, backend: 'mongodb' });
    assert.equal(metrics.totalFiles, 1);
    assert.equal(metrics.totalLoc, 2);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});
