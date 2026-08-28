import assert from 'node:assert/strict';
import test from 'node:test';

import { mutationWorkerCount, parseQualificationArgs, qualificationReadiness }
  from '../commands/qualification-cli.mjs';

function assertQualificationIsCurrent(status) {
  assert.equal(status.promotion.ready, true);
  assert.deepEqual(status.promotion.blockers, []);
}

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
  assert.throws(() => mutationWorkerCount({ mutations: [{ ...calibration.mutations[0],
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

test('sequential L2 qualification waits for a promoted L1 baseline', () => {
  assert.throws(() => qualificationReadiness('ecommerce', 2),
    /requires exactly one promoted L1 base/);
});

test('qualification status uses the exact progression check subset', () => {
  const status = qualificationReadiness('ecommerce', 3,
    'ecommerce.progression-catalog@1.0.0');
  assert.equal(status.defectChecks.totalChecks, 112);
  assert.equal(status.defectChecks.totalPoints, 199);
  assert(status.defectChecks.stacks.every(stack => stack.coveredChecks === 112
    && stack.coveredPoints === 199 && stack.missingChecks.length === 0));
  assert(status.commands.filter(command => command.startsWith('qualify-reference'))
    .every(command => command.includes('--feature-catalog ecommerce.questlines@1.0.0')));
  assert(status.commands.filter(command => command.includes('--mutations'))
    .every(command => command.includes('--mutation-workers 4')
      && command.includes('--release-candidate')));
  assert.equal(status.commands.filter(command => command.startsWith('qualify-reference')).length, 3);
  assert.equal(status.promotion.blockers.some(blocker =>
    blocker.code === 'defect_check_coverage_incomplete'), false);
});

test('the cumulative release discloses complete defect coverage at every promoted level', () => {
  for (const level of [1, 2, 3]) {
    const status = qualificationReadiness('ecommerce', level,
      'ecommerce.progression-l1-l3@1.1.0');
    assert.equal(status.scope.recipe.id, 'ecommerce.progression-l1-l3');
    assert.equal(status.defectChecks.totalChecks, 112);
    assert.equal(status.defectChecks.totalPoints, 199);
    assert(status.defectChecks.stacks.every(item => item.coveredChecks === 112
      && item.coveredPoints === 199
    && item.missingChecks.length === 0));
    assertQualificationIsCurrent(status);
    assert.equal(status.commands.every(command => command.includes('--level 3')), true);
    assert.equal(status.requiredEvidence.length, 7);
  }
});

test('qualification resolves the pending modular L1 release exactly and by default', () => {
  const parsed = parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1', '--recipe', 'ecommerce.l1-modular@2.5.0']);
  assert.equal(parsed.recipe, 'ecommerce.l1-modular@2.5.0');
  const status = qualificationReadiness(parsed.track, parsed.level, parsed.recipe);
  assert.equal(status.scope.recipe.version, '2.5.0');
  assert.equal(status.scope.calibration.version, '2.5.0');
  assert.equal(status.launch.ok, true);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.promotion.ready, false);
  assert(status.promotion.governance.some(item => item.path === 'promotion.status'
    && item.state === 'candidate' && item.target === 'promoted'));
  assert(status.commands.every(command => command.includes('--recipe ecommerce.l1-modular@2.5.0')));
  assert.equal(qualificationReadiness('ecommerce', 1).scope.recipe.version, '2.5.0');
  assert.throws(() => qualificationReadiness('ecommerce', 1, 'ecommerce.l1-modular@2.3.0'),
    /no recipe release|retired|requires exactly one catalogued/);
  assert.throws(() => qualificationReadiness('ecommerce', 1, 'ecommerce.l1-standard@1.1.0'),
    /no recipe release|retired|requires exactly one catalogued/);
});

test('pending modular L2 cannot bypass its L1 qualification dependency', () => {
  assert.throws(() => qualificationReadiness('ecommerce', 2, 'ecommerce.l2-standard@1.6.0'),
    /requires exactly one promoted L1 base/);
  assert.throws(() => qualificationReadiness('ecommerce', 2),
    /requires exactly one promoted L1 base/);
  assert.throws(() => qualificationReadiness('ecommerce', 2, 'ecommerce.l2-standard@1.2.0'),
    /no recipe release|retired|requires exactly one catalogued/);
});
