import assert from 'node:assert/strict';
import test from 'node:test';

import { mutationWorkerCount, parseQualificationArgs, qualificationReadiness }
  from '../commands/qualification-cli.js';

test('mutation worker count uses only mutations selected by calibration', () => {
  const calibration = { mutations: [{ backend: 'postgres', path: 'manifest.json',
    targets: [{ id: 'selected' }] }] };
  const manifest = { mutations: [
    { id: 'selected', scenario: 'one.json' },
    { id: 'not-selected-a', scenario: 'two.json' },
    { id: 'not-selected-b', scenario: 'three.json' },
    { id: 'not-selected-c', scenario: 'four.json' },
  ] };
  assert.equal(mutationWorkerCount(calibration, 'postgres', () => manifest), 1);
  const calibrationMutation = calibration.mutations[0];
  assert(calibrationMutation);
  assert.throws(() => mutationWorkerCount({ mutations: [{ ...calibrationMutation,
    targets: [{ id: 'missing' }] }] }, 'postgres', () => manifest), /missing mutations/);
});

test('mutation workers split defects even when they use one scenario', () => {
  const calibration = { mutations: [{ backend: 'postgres', path: 'manifest.json',
    targets: ['a', 'b', 'c', 'd', 'e'].map(id => ({ id })) }] };
  const manifest = { scenario: 'shared.json',
    mutations: ['a', 'b', 'c', 'd', 'e'].map(id => ({ id })) };
  assert.equal(mutationWorkerCount(calibration, 'postgres', () => manifest), 4);
});

test('pending L1 qualification lists the required evidence without writing', () => {
  const status = qualificationReadiness('ecommerce', 1);
  assert.match(status.scope.calibration.sha256, /^[a-f0-9]{64}$/);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.commands.length, 4);
  assert.equal(status.budgetPreparation.required, false);
  assert.deepEqual(status.budgetPreparation.commands, []);
  assert.equal(status.launch.ok, true);
  assert.deepEqual(status.launch.blockers, []);
  assert.equal(status.promotion.ready, false);
  assert.equal(status.promotion.blockers.filter(item => item.code === 'evidence_missing').length, 7);
  assert(status.promotion.governance.some(item => item.path === 'recipe.state'
    && item.state === 'draft' && item.target === 'qualified'));
  assert.equal(status.promotion.blockers.some(item => item.code === 'source_not_promoted'), false);
});

test('qualification status rejects ambiguous or undeclared scope', () => {
  assert.deepEqual(parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1']), { command: 'status', track: 'ecommerce', level: 1 });
  assert.throws(() => qualificationReadiness('ecommerce', 3), /has no L3 calibration/);
  assert.throws(() => qualificationReadiness('ecommerce', 4), /not declared/);
  assert.throws(() => parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce']), /usage/);
});

test('sequential L2 qualification uses its exact current L1 base', () => {
  assert.equal(qualificationReadiness('ecommerce', 2).scope.recipe.version, '1.6.0');
});

test('qualification status uses the exact progression check subset', () => {
  const status = qualificationReadiness('ecommerce', 3,
    'ecommerce.progression-depth3@2.0.1');
  assert.equal(status.defectChecks.totalChecks, 97);
  assert.equal(status.defectChecks.totalPoints, 162);
  assert(status.defectChecks.stacks.every(stack => stack.coveredChecks === 97
    && stack.coveredPoints === 162 && stack.missingChecks.length === 0));
  assert(status.commands.filter(command => command.startsWith('qualify-reference'))
    .every(command => command.includes('--feature-catalog ecommerce.questlines@2.0.1')));
  assert(status.commands.filter(command => command.includes('--mutations'))
    .every(command => command.includes('--mutation-workers 4')
      && command.includes('--release-candidate')));
  assert.equal(status.commands.filter(command => command.startsWith('qualify-reference')).length, 3);
  assert.equal(status.promotion.blockers.some(blocker =>
    blocker.code === 'defect_check_coverage_incomplete'), false);
});

test('the cumulative release is ready at every covered level', () => {
  for (const level of [1, 2, 3]) {
    const status = qualificationReadiness('ecommerce', level,
      'ecommerce.progression-depth3@2.0.1');
    assert.equal(status.scope.recipe.id, 'ecommerce.progression-depth3');
    assert.equal(status.defectChecks.totalChecks, 97);
    assert.equal(status.defectChecks.totalPoints, 162);
    assert(status.defectChecks.stacks.every(item => item.coveredChecks === 97
      && item.coveredPoints === 162
    && item.missingChecks.length === 0));
    assert.equal(status.launch.ok, true);
    assert.equal(status.promotion.ready, true);
    assert.deepEqual(status.promotion.blockers, []);
    assert.equal(status.commands.every(command => command.includes('--level 3')), true);
    assert.equal(status.requiredEvidence.length, 7);
  }
});

test('qualification resolves the pending sequential L1 release exactly and by default', () => {
  const parsed = parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1', '--recipe', 'ecommerce.sequential-l1@2.5.0']);
  assert.equal(parsed.command, 'status');
  assert.equal(parsed.recipe, 'ecommerce.sequential-l1@2.5.0');
  const track = parsed.track;
  assert(track);
  const level = parsed.level;
  assert(level !== null);
  const recipe = parsed.recipe;
  assert(recipe);
  const status = qualificationReadiness(track, level, recipe);
  assert.equal(status.scope.recipe.version, '2.5.0');
  assert.equal(status.scope.calibration.version, '2.5.0');
  assert.equal(status.launch.ok, true);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.promotion.ready, false);
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'candidate' && item.target === 'promoted'));
  assert(status.commands.every(command => command.includes('--recipe ecommerce.sequential-l1@2.5.0')));
  const defaultStatus = qualificationReadiness('ecommerce', 1);
  assert.equal(defaultStatus.scope.recipe.version, '2.5.0');
  assert(defaultStatus.commands.every(command =>
    command.includes('--recipe ecommerce.sequential-l1@2.5.0')));
});

test('pending modular L2 resolves only the current exact recipe', () => {
  assert.equal(qualificationReadiness('ecommerce', 2,
    'ecommerce.sequential-l2@1.6.0').scope.recipe.version, '1.6.0');
  assert.equal(qualificationReadiness('ecommerce', 2).scope.recipe.version, '1.6.0');
});
