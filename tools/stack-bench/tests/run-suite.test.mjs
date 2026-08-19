import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { childFailureDetail, clearPreviousGradeOutputs, findMutationBackups, selectObservationScope,
  applicationFailureTotals, resetFailureOutcome, suitesForRecipe,
  runGraderChild, verifyReseedProbe } from '../commands/run-suite.mjs';
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

test('a new grade removes only prior outputs that it owns', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grade-cleanup-'));
  try {
    mkdirSync(join(root, 'failure-media'), { recursive: true });
    writeFileSync(join(root, 'bundle.json'), 'old');
    writeFileSync(join(root, 'grading-features.json'), 'old');
    writeFileSync(join(root, 'grader-features.stdout.log'), 'old');
    writeFileSync(join(root, 'grader-features.stderr.log'), 'old');
    writeFileSync(join(root, 'operator-notes.txt'), 'keep');
    clearPreviousGradeOutputs(root, [{ id: 'features' }]);
    assert.equal(existsSync(join(root, 'bundle.json')), false);
    assert.equal(existsSync(join(root, 'grading-features.json')), false);
    assert.equal(existsSync(join(root, 'grader-features.stdout.log')), false);
    assert.equal(existsSync(join(root, 'grader-features.stderr.log')), false);
    assert.equal(existsSync(join(root, 'failure-media')), false);
    assert.equal(readFileSync(join(root, 'operator-notes.txt'), 'utf8'), 'keep');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('observed-only scope is modular, disjoint, and contributes no score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.4.0');
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
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.4.0');
  const suites = suitesForRecipe(track, binding);

  assert.match(suites.find(suite => suite.id === 'duplicate-checkout').spec,
    /01-duplicate-checkout-2\.3\.0\.json$/);
  assert.equal(suites.some(suite => suite.inherited), false);
});

test('hardened modular grading isolates the four direct server checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.4.0');
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
  const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.4.0');
  const suites = suitesForRecipe(track, binding);

  const inherited = suites.filter(suite => suite.inherited);
  assert.equal(inherited.length, 11);
  assert(inherited.every(suite => suite.fromLevel === 1));
  assert.equal(suites.filter(suite => !suite.inherited).length, 10);
});

test('cumulative ownership survives inherited execution id renames', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-execution-ownership-'));
  const root = join(temp, 'ecommerce');
  try {
    cpSync(ECOMMERCE, root, { recursive: true });
    const recipe = join(root, 'composition', 'recipes', 'l2-standard-1.4.0.json');
    const value = JSON.parse(readFileSync(recipe, 'utf8'));
    for (const execution of value.execution) {
      if (execution.id.endsWith('@L1')) execution.id = `${execution.id.slice(0, -3)}-base`;
    }
    writeFileSync(recipe, `${JSON.stringify(value, null, 2)}\n`);
    const track = { ...loadTrack('ecommerce'), dir: root,
      suites: JSON.parse(readFileSync(join(root, 'track.json'), 'utf8')).suites };
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.4.0');
    const suites = suitesForRecipe(track, binding);
    const inherited = suites.filter(suite => suite.inherited);

    assert.equal(inherited.length, 11);
    assert(inherited.every(suite => suite.id.endsWith('-base')));
    assert(inherited.every(suite => suite.fromLevel === 1));
    assert.deepEqual(suites.filter(suite => !suite.inherited).map(suite => suite.id),
      ['features-existing@L2', 'low-stock@L2', 'invariants-existing@L2',
        'queue-warehouse@L2', 'transfer-totals@L2', 'paid-price-history@L2',
        'live-price@L2', 'self-contained@L2', 'strengthened@L2', 'server-actions@L2']);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});
