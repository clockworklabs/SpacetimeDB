import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBoundRecipeTaskRequest } from '../recipe-selection.mjs';
import { resolveRecipeRelease } from '../recipe-release.mjs';
import { childFailureDetail, selectObservationScope, suitesForRecipe } from '../run-suite.mjs';
import { loadTrack } from '../tracks.mjs';

const ECOMMERCE = join(import.meta.dirname, '..', 'tracks', 'ecommerce');

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

test('observed-only scope is modular, disjoint, and contributes no score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.3.0');
  const selected = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    observedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
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
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.3.0');
  const suites = suitesForRecipe(track, binding);

  assert.match(suites.find(suite => suite.id === 'duplicate-checkout').spec,
    /01-duplicate-checkout-2\.3\.0\.json$/);
  assert.equal(suites.some(suite => suite.inherited), false);
});

test('hardened modular grading isolates the four direct server checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.3.0');
  const suites = suitesForRecipe(track, binding);

  assert.match(suites.find(suite => suite.id === 'server-actions').spec,
    /01-server-actions-2\.1\.0\.json$/);
  assert.deepEqual(binding.release.checkCatalog
    .filter(check => check.executionId === 'server-actions')
    .map(check => String(check.criterionId)), ['101a', '102a', '103a', '104a']);
});

test('recipe execution keeps inherited suites out of the current-level score', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.2.0');
  const suites = suitesForRecipe(track, binding);

  assert.deepEqual(suites.filter(suite => suite.inherited)
    .map(suite => ({ id: suite.id, fromLevel: suite.fromLevel })), [
    { id: 'invariants@L1', fromLevel: 1 },
    { id: 'systems@L1', fromLevel: 1 },
  ]);
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
