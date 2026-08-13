import assert from 'node:assert/strict';
import test from 'node:test';

import { parseQualificationArgs, qualificationReadiness } from '../qualification-cli.mjs';

test('qualification status lists exact evidence and launch readiness without writing', () => {
  const status = qualificationReadiness('ecommerce', 1);
  assert.match(status.scope.calibration.sha256, /^[a-f0-9]{64}$/);
  assert.equal(status.requiredEvidence.length, 13);
  assert.equal(status.commands.length, 7);
  assert.equal(status.budgetPreparation.required, false);
  assert.deepEqual(status.budgetPreparation.commands, []);
  assert.equal(status.launch.ok, true);
  assert.deepEqual(status.launch.blockers, []);
  assert.equal(status.promotion.ready, false);
  assert(status.promotion.blockers.some(item => item.code === 'evidence_missing'));
  assert(status.promotion.governance.some(item => item.path === 'recipe.state'
    && item.state === 'draft' && item.target === 'qualified'));
  assert.equal(status.promotion.blockers.some(item => item.code === 'source_not_promoted'), false);
});

test('qualification status rejects ambiguous or unvalidated scope', () => {
  assert.deepEqual(parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1']), { command: 'status', track: 'ecommerce', level: 1 });
  assert.throws(() => qualificationReadiness('ecommerce', 3), /not validated/);
  assert.throws(() => parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce']), /usage/);
});
