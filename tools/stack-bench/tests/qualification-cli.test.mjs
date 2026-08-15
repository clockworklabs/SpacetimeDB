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
  assert.equal(status.promotion.ready, true);
  assert.deepEqual(status.promotion.blockers, []);
  assert(status.promotion.governance.some(item => item.path === 'recipe.state'
    && item.state === 'qualified' && item.target === 'qualified'));
  assert.equal(status.promotion.blockers.some(item => item.code === 'source_not_promoted'), false);
});

test('qualification status rejects ambiguous or undeclared scope', () => {
  assert.deepEqual(parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1']), { command: 'status', track: 'ecommerce', level: 1 });
  assert.throws(() => qualificationReadiness('ecommerce', 3), /no recipe release/);
  assert.throws(() => qualificationReadiness('ecommerce', 4), /not declared/);
  assert.throws(() => parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce']), /usage/);
});

test('qualification status exposes the exact runner required by a recipe', () => {
  const status = qualificationReadiness('ecommerce', 2);
  assert.deepEqual(status.scope.runner, {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
  });
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'promoted' && item.target === 'promoted'));
  assert.equal(status.promotion.ready, true);
  assert.deepEqual(status.promotion.blockers, []);
  assert.equal(status.launch.ok, true);
});

test('qualification can inspect an exact candidate without changing the L1 default', () => {
  const parsed = parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1', '--recipe', 'ecommerce.l1-standard@1.1.0']);
  assert.equal(parsed.recipe, 'ecommerce.l1-standard@1.1.0');
  const status = qualificationReadiness(parsed.track, parsed.level, parsed.recipe);
  assert.equal(status.scope.recipe.version, '1.1.0');
  assert.equal(status.scope.calibration.version, '1.1.0');
  assert.equal(status.launch.ok, true);
  assert.equal(status.promotion.ready, false);
  assert.equal(status.promotion.blockers.filter(item => item.code === 'evidence_missing').length, 13);
  assert(status.commands.every(command => command.includes('--recipe ecommerce.l1-standard@1.1.0')));
  assert.equal(qualificationReadiness('ecommerce', 1).scope.recipe.version, '1.0.0');
});
