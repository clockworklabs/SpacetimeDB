import assert from 'node:assert/strict';
import test from 'node:test';

import { parseQualificationArgs, qualificationReadiness } from '../commands/qualification-cli.mjs';

test('qualification status lists exact evidence and launch readiness without writing', () => {
  const status = qualificationReadiness('ecommerce', 1);
  assert.match(status.scope.calibration.sha256, /^[a-f0-9]{64}$/);
  assert.equal(status.requiredEvidence.length, 7);
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

test('qualification status separates launch readiness from incomplete defect coverage', () => {
  const status = qualificationReadiness('ecommerce', 2);
  assert.deepEqual(status.scope.runner, {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
  });
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'promoted' && item.target === 'promoted'));
  assert.equal(status.promotion.ready, false);
  assert.equal(status.promotion.blockers.length, 3);
  assert(status.promotion.blockers.every(item => item.code === 'defect_check_coverage_incomplete'));
  assert.equal(status.launch.ok, true);
});

test('the promoted L1 release discloses complete defect coverage and exact evidence', () => {
  const status = qualificationReadiness('ecommerce', 1, 'ecommerce.l1-modular@2.4.0');
  assert.equal(status.defectChecks.totalChecks, 46);
  assert.equal(status.defectChecks.totalPoints, 58);
  assert.deepEqual(status.defectChecks.stacks.map(item => [item.stack, item.coveredChecks]), [
    ['mongodb', 46], ['postgres', 46], ['spacetime', 46],
  ]);
  assert(status.defectChecks.stacks.every(item => item.coveredPoints === 58
    && item.missingChecks.length === 0));
  assert.equal(status.promotion.ready, true);
  assert.equal(status.promotion.blockers
    .filter(item => item.code === 'defect_check_coverage_incomplete').length, 0);
  assert.deepEqual(status.promotion.blockers, []);
  assert.equal(status.requiredEvidence.length, 7);
});

test('qualification resolves the promoted modular L1 release exactly and by default', () => {
  const parsed = parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1', '--recipe', 'ecommerce.l1-modular@2.4.0']);
  assert.equal(parsed.recipe, 'ecommerce.l1-modular@2.4.0');
  const status = qualificationReadiness(parsed.track, parsed.level, parsed.recipe);
  assert.equal(status.scope.recipe.version, '2.4.0');
  assert.equal(status.scope.calibration.version, '2.4.0');
  assert.equal(status.launch.ok, true);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.promotion.ready, true);
  assert.deepEqual(status.promotion.blockers, []);
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'promoted' && item.target === 'promoted'));
  assert(status.commands.every(command => command.includes('--recipe ecommerce.l1-modular@2.4.0')));
  assert.equal(qualificationReadiness('ecommerce', 1).scope.recipe.version, '2.4.0');
  assert.throws(() => qualificationReadiness('ecommerce', 1, 'ecommerce.l1-modular@2.3.0'),
    /no recipe release|retired|requires exactly one catalogued/);
  assert.throws(() => qualificationReadiness('ecommerce', 1, 'ecommerce.l1-standard@1.1.0'),
    /no recipe release|retired|requires exactly one catalogued/);
});

test('qualification resolves the promoted modular L2 release exactly and by default', () => {
  const status = qualificationReadiness('ecommerce', 2, 'ecommerce.l2-standard@1.4.0');
  assert.equal(status.scope.recipe.version, '1.4.0');
  assert.equal(status.scope.calibration.version, '1.4.0');
  assert.equal(status.launch.ok, true);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.promotion.ready, false);
  assert.deepEqual(status.promotion.blockers.map(item => [item.code, item.path]), [
    ['defect_check_coverage_incomplete', 'defectChecks.mongodb'],
    ['defect_check_coverage_incomplete', 'defectChecks.postgres'],
    ['defect_check_coverage_incomplete', 'defectChecks.spacetime'],
  ]);
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'promoted' && item.target === 'promoted'));
  assert(status.commands.every(command => command.includes('--recipe ecommerce.l2-standard@1.4.0')));
  assert.equal(qualificationReadiness('ecommerce', 2).scope.recipe.version, '1.4.0');
  assert.throws(() => qualificationReadiness('ecommerce', 2, 'ecommerce.l2-standard@1.2.0'),
    /no recipe release|retired|requires exactly one catalogued/);
});
