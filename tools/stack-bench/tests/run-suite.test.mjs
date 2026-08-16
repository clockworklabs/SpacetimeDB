import assert from 'node:assert/strict';
import test from 'node:test';

import { createBoundRecipeTaskRequest } from '../recipe-selection.mjs';
import { resolveRecipeRelease } from '../recipe-release.mjs';
import { childFailureDetail, selectObservationScope } from '../run-suite.mjs';
import { loadTrack } from '../tracks.mjs';

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

test('probe observation scope is modular, disjoint, and contributes no requested score', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.0.0');
  const selected = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    probedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  });
  const requested = selectObservationScope(selected, 'requested');
  const probe = selectObservationScope(selected, 'probe');
  assert.deepEqual(probe.checks, selected.selection.probeChecks);
  assert.equal(probe.scoredPoints, 0);
  assert(probe.observedPoints > 0);
  assert.equal(probe.checks.some(check => requested.checks.includes(check)), false);
  assert.throws(() => selectObservationScope(
    createBoundRecipeTaskRequest(binding, { featureIds: ['ecommerce.feature.accounts'] }),
    'probe'), /scope is empty/);
});
