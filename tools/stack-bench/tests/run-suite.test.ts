import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { isModularRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { attachRegressionScope, childFailureDetail, clearPreviousGradeOutputs, findMutationBackups, selectObservationScope,
  applicationFailureTotals, checkDatabaseProvenance, codeMetrics, resetFailureOutcome, suitesForRecipe,
  checkRuntimeDatabaseProvenance, databaseProvenanceFailure, writeApplicationDatabaseMarker,
  contractLintArgv, databaseLeaseForGrading, databaseNameForGrading, runGraderChild,
  verifyApplicationProbe, waitForApplicationProbe }
  from '../commands/run-suite.js';
import { loadTrack } from '../src/composition/tracks.js';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const ECOMMERCE = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function readJsonRecord(path: string): JsonRecord {
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  if (!isRecord(value)) throw new Error(`${path} must contain an object`);
  return value;
}

function recordArray(value: JsonRecord, key: string): JsonRecord[] {
  const entries = value[key];
  if (!Array.isArray(entries) || entries.some(entry => !isRecord(entry))) {
    throw new Error(`${key} must be an array of objects`);
  }
  return entries;
}

function promoteSequentialL1(root: string): void {
  const promotionsPath = join(root, 'composition', 'promotions.json');
  const promotions = readJsonRecord(promotionsPath);
  const entries = recordArray(promotions, 'entries');
  const entry = entries.find(candidate => {
    const recipe = candidate.recipe;
    return candidate.alias === 'L1' && isRecord(recipe)
      && recipe.id === 'ecommerce.sequential-l1' && recipe.version === '2.5.0';
  });
  if (!entry) throw new Error('L1 promotion entry is missing');
  entry.status = 'promoted';
  writeFileSync(promotionsPath, `${JSON.stringify(promotions, null, 2)}\n`);
  const recipePath = join(root, 'composition', 'recipes', 'sequential-l1-2.5.0.json');
  const recipe = readJsonRecord(recipePath);
  recipe.state = 'qualified';
  writeFileSync(recipePath, `${JSON.stringify(recipe, null, 2)}\n`);
}

function sequentialL2Track() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-l2-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  promoteSequentialL1(root);
  return { temp, track: { ...loadTrack('ecommerce'), dir: root } };
}

test('contract lint receives the selected credential aliases', () => {
  const aliases = { 'stackbench-admin-2026': 'store-admin-2026' };
  const argv = contractLintArgv({
    url: 'http://app', level: '1', track: 'ecommerce', label: 'attempt',
    out: '/results', bundleArtifactId: 'attempt', credentialAliases: aliases,
  });
  assert.equal(argv[argv.indexOf('--credential-aliases-json') + 1], JSON.stringify(aliases));
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

test('child diagnostics prefer process stderr over generated command text', () => {
  const detail = childFailureDetail({
    stderr: 'hosted application port 6301 still has a listener',
    message: 'Command failed: docker exec generated-app sh -lc <large command>',
  });
  assert.equal(detail, 'hosted application port 6301 still has a listener');
  assert.doesNotMatch(detail, /docker exec/);
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

test('SpacetimeDB grading uses the authenticated module lease', () => {
  const lease = databaseLeaseForGrading('spacetime', {
    STACK_BENCH_LEASE: 'private/lease.json',
    STACK_BENCH_LEASE_TOKEN: 'secret-token',
  }, {
    readLease: () => createBackendLease({ runId: 'grading-test', backend: 'spacetime',
      track: 'ecommerce', runIndex: 0, module: 'app_ecommerce_run0',
      serverUri: 'http://127.0.0.1:3210', dataDir: join(tmpdir(), 'stack-bench-spacetime-test') }),
  });
  assert.equal(lease?.resources.module, 'app_ecommerce_run0');
  assert.equal(lease?.resources.serverUri, 'http://127.0.0.1:3210');
});

test('database provenance parses the port instead of accepting a matching substring', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-database-provenance-'));
  try {
    writeFileSync(join(temp, 'server.js'),
      "const url = 'mongodb://localhost:6537/app_ecom_run0';\n");
    assert.equal(checkDatabaseProvenance({ app: temp, backend: 'mongodb' }).ok, true);
    writeFileSync(join(temp, 'server.js'),
      "const url = 'mongodb://localhost:16537/app_ecom_run0';\n");
    assert.equal(checkDatabaseProvenance({ app: temp, backend: 'mongodb' }).ok, false);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test('runtime database marker uses the declared application action', async () => {
  const track = loadTrack('ecommerce');
  const requests: Array<{ url: string; body: JsonRecord }> = [];
  const written = await writeApplicationDatabaseMarker(
    { backend: 'postgres', url: 'http://shop.test' }, track, track.databaseProvenance,
    async (url, init) => {
      requests.push({ url, body: JSON.parse(init.body) as JsonRecord });
      return { ok: true, status: 201 };
    });
  assert(written.ok);
  const request = requests[0];
  assert(request);
  assert.equal(request.url, 'http://shop.test/api/auth/signup');
  assert.equal(request.body.password, 'stack-bench-provenance-password');
  assert.equal(request.body.username, written.marker);
  assert.match(written.marker, /^sb[a-f0-9]{16}$/);

  const refused = await writeApplicationDatabaseMarker(
    { backend: 'postgres', url: 'http://shop.test' }, track, track.databaseProvenance,
    async () => ({ ok: false, status: 422 }));
  assert.deepEqual(refused, { ok: false, marker: null,
    reason: 'application provenance action returned HTTP 422' });
});

test('runtime database proof reports command failures as harness failures', () => {
  assert.deepEqual(databaseProvenanceFailure(new Error('docker command failed')), {
    kind: 'harness_failure',
    phase: 'database-provenance',
    reason: 'runtime database provenance failed: docker command failed',
  });
});

test('runtime database proof requires an authenticated lease', () => {
  assert.deepEqual(checkRuntimeDatabaseProvenance({ backend: 'spacetime' }), {
    ok: null,
    verified: false,
    reason: 'standalone grading has no authenticated database lease',
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

test('reset readiness probes the public application', async () => {
  let requests = 0;
  const result = await verifyApplicationProbe('http://app', {
    fetchImpl: async () => {
      requests += 1;
      return { ok: true, status: 200 };
    },
  });
  assert.deepEqual(result, { ok: true, detail: null });
  assert.equal(requests, 1);
});

test('reset readiness rejects an unhealthy public application', async () => {
  const result = await verifyApplicationProbe('http://app', {
    fetchImpl: async () => ({ ok: false, status: 503 }),
  });
  assert.deepEqual(result, {
    ok: false,
    detail: 'application returned HTTP 503',
  });
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
});

test('application readiness returns the last failure after its bounded attempts', async () => {
  let calls = 0;
  const result = await waitForApplicationProbe('http://app', {
    attempts: 2, intervalMs: 0,
    probe: async () => { calls += 1; return { ok: false, detail: 'not ready' }; },
    sleepImpl: async () => {},
  });
  assert.deepEqual(result, { ok: false, detail: 'not ready' });
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
    mkdirSync(join(root, 'database-provenance'), { recursive: true });
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
    assert.equal(existsSync(join(root, 'database-provenance')), false);
    assert.equal(readFileSync(join(root, 'operator-notes.txt'), 'utf8'), 'keep');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('observed-only scope is modular, disjoint, and contributes no score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.sequential-l1@2.5.0');
  const selected = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    observedSpecifications: ['ecommerce.spec.state-durability@1.1.0'],
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
  assert.equal(observed.checks.some(check => scored.checks.includes(check)), false);
  assert.throws(() => selectObservationScope(
    createBoundRecipeTaskRequest(binding, { featureIds: ['ecommerce.feature.accounts'] }),
    'observed'), /scope is empty/);
});

test('recipe-bound grading uses the recipe execution sources', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
  const suites = suitesForRecipe(track, binding);

  const duplicateCheckout = suites.find(suite => suite.id === 'duplicate-checkout');
  assert(duplicateCheckout);
  assert.match(duplicateCheckout.spec,
    /01-duplicate-checkout-2\.3\.0\.json$/);
  assert.equal(suites.some(suite => suite.inherited), false);
});

test('hardened modular grading isolates the four direct server checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
  const suites = suitesForRecipe(track, binding);

  const expected = new Map([
    ['purchase-session', '101a'], ['purchase-attribution', '102a'],
    ['admin-write', '103a'], ['server-price', '104a'],
  ]);
  for (const [executionId, criterionId] of expected) {
    assert(suites.some(suite => suite.id === executionId));
    const check = binding.release.checkCatalog.find(candidate => candidate.executionId === executionId);
    assert(check);
    assert.equal(check.criterionId, criterionId);
  }
});

test('recipe execution keeps inherited suites out of the current-level score', () => {
  const { temp, track } = sequentialL2Track();
  try {
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2@1.6.0');
    const suites = suitesForRecipe(track, binding);
    const inherited = suites.filter(suite => suite.inherited);
    assert.equal(inherited.length, 31);
    assert(inherited.every(suite => suite.fromLevel === 1));
    assert.equal(suites.filter(suite => !suite.inherited).length, 10);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('L2 grading rechecks the exact selected L1 score without adding it to L2 points', () => {
  const { temp, track } = sequentialL2Track();
  try {
    const l1 = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
    const l2 = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2@1.6.0');
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
    assert(isModularRecipeTaskRequest(prior));
    assert(isModularRecipeTaskRequest(current));
    const scope = attachRegressionScope(current.selection, l2, suitesForRecipe(track, l2),
      prior.selection.scoredChecks.map(check => check.stableKey));
    assert(scope);
    assert(scope.regressionChecks);
    assert(scope.evaluationSha256);
    assert.equal(scope.scoredPoints, 59);
    assert.equal(scope.regressionPoints, 58);
    assert.equal(scope.regressionChecks.length, prior.selection.scoredChecks.length);
    assert.equal(scope.checks.length,
      current.selection.scoredChecks.length + prior.selection.scoredChecks.length);
    assert.match(scope.evaluationSha256, /^[a-f0-9]{64}$/);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('cumulative ownership survives inherited execution id renames', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-execution-ownership-'));
  const root = join(temp, 'ecommerce');
  try {
    cpSync(ECOMMERCE, root, { recursive: true });
    const recipe = join(root, 'composition', 'recipes', 'sequential-l2-1.6.0.json');
    const value = readJsonRecord(recipe);
    for (const execution of recordArray(value, 'execution')) {
      if (typeof execution.id !== 'string') throw new Error('recipe execution must have an id');
      if (execution.id.endsWith('@L1')) execution.id = `${execution.id.slice(0, -3)}-base`;
    }
    writeFileSync(recipe, `${JSON.stringify(value, null, 2)}\n`);
    const track = { ...loadTrack('ecommerce'), dir: root };
    promoteSequentialL1(root);
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2@1.6.0');
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
