import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCalibrationFile } from '../calibration-compiler.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = join(TRACK, 'composition', 'recipes', 'l2-standard-1.3.0.json');
const CALIBRATION = join(TRACK, 'composition', 'calibrations', 'l2-standard-1.3.0.json');

test('L2 1.3 has a source-bound draft calibration for the complete cumulative recipe', () => {
  const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK });
  const plan = compileCalibrationFile(CALIBRATION, {
    trackRoot: TRACK,
    stackBenchRoot: ROOT,
    release,
  });

  assert.deepEqual({ id: release.id, version: release.version, state: release.state,
    checks: release.checkCatalog.length, points: release.scoring.points }, {
    id: 'ecommerce.l2-standard', version: '1.3.0', state: 'draft', checks: 76, points: 111,
  });
  assert.equal(plan.state, 'draft');
  assert.equal(plan.recipe.contentSha256, release.contentSha256);
  assert.equal(plan.fixture.sourceSha256, release.components.fixture.sha256);
  assert.deepEqual(plan.references.entries.map(entry => [entry.backend, entry.status]), [
    ['mongodb', 'candidate'], ['postgres', 'candidate'], ['spacetime', 'candidate'],
  ]);
  assert.deepEqual(plan.mutations.map(entry => [entry.backend, entry.status, entry.targets.length]), [
    ['mongodb', 'candidate', 6], ['postgres', 'candidate', 6], ['spacetime', 'candidate', 7],
  ]);
  for (const mutation of plan.mutations) {
    const targets = new Map(mutation.targets.map(target => [target.id, target.stableKeys]));
    assert.deepEqual(targets.get('customer-can-ship-order-replay'),
      ['ecommerce.operations-access.fulfilment-queue.1e']);
    assert.deepEqual(targets.get('customer-can-ship-order-direct'),
      ['ecommerce.operations-access.operator-authorization.201c']);
    assert.deepEqual(targets.get('customer-can-cancel-foreign-order'),
      ['ecommerce.operations-access.order-owner.204a']);
    assert.equal([...targets.values()].some(stableKeys =>
      stableKeys.includes('ecommerce.inventory-operations.stock-conservation.202d')), false);
    const lastUnitTargets = [...targets.values()].find(stableKeys =>
      stableKeys.includes('ecommerce.spec.concurrency-safety.last-unit.201a'));
    assert.deepEqual(lastUnitTargets, [
      'ecommerce.spec.concurrency-safety.last-unit.201a',
      'ecommerce.spec.concurrency-safety.last-unit.201b',
      'ecommerce.spec.concurrency-safety.last-unit.201c',
    ]);
  }
  assert.equal(plan.controls.length, 8);
  const revenue = plan.controls.find(control =>
    control.stableKey === 'ecommerce.spec.concurrency-safety.last-unit.201c');
  assert.deepEqual(revenue, {
    stableKey: 'ecommerce.spec.concurrency-safety.last-unit.201c',
    role: 'promotion-gate',
    promotionPolicy: 'must-pass-reference-and-kill-declared-mutant',
    mutationTargets: [
      'mongodb:oversell-unguarded-decrement',
      'postgres:oversell-no-row-lock',
      'spacetime:purchase-does-not-reserve-stock-last-unit',
    ],
  });
  assert.equal(plan.qualification.evidence.length, 0);
  assert.deepEqual(plan.qualification.stacks.map(stack => stack.status),
    ['candidate', 'candidate', 'candidate']);
  assert.match(plan.qualificationSha256, /^[a-f0-9]{64}$/);
  assert.match(plan.contentSha256, /^[a-f0-9]{64}$/);
});
