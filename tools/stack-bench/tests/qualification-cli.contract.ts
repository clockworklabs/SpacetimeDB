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
  assert.match(status.scope.calibration.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.commands.length, 4);
  assert.equal(status.budgetPreparation.required, false);
  assert.deepEqual(status.budgetPreparation.commands, []);
  assert.equal(status.launch.ok, true);
  assert.deepEqual(status.launch.blockers, []);
  assert.equal(status.qualification.ready, false);
  assert.equal(status.qualification.blockers.filter(item => item.code === 'evidence_missing').length, 7);
});

test('qualification status rejects ambiguous or undeclared scope', () => {
  assert.deepEqual(parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1']), { command: 'status', track: 'ecommerce', level: 1 });
  assert.throws(() => qualificationReadiness('ecommerce', 3), /has no L3 calibration/);
  assert.throws(() => qualificationReadiness('ecommerce', 3,
    'ecommerce.progression-catalog'), /has no L3 calibration/);
  assert.throws(() => qualificationReadiness('ecommerce', 4), /not declared/);
  assert.throws(() => parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce']), /usage/);
});

test('sequential L2 qualification uses its exact current L1 base', () => {
  assert.equal(qualificationReadiness('ecommerce', 2).scope.recipe.id, 'ecommerce.sequential-l2');
});

test('qualification resolves the pending sequential L1 release exactly and by default', () => {
  const parsed = parseQualificationArgs(['node', 'qualification-cli.mjs', 'status',
    '--track', 'ecommerce', '--level', '1', '--recipe', 'ecommerce.sequential-l1']);
  assert.equal(parsed.command, 'status');
  assert.equal(parsed.recipe, 'ecommerce.sequential-l1');
  const track = parsed.track;
  assert(track);
  const level = parsed.level;
  assert(level !== null);
  const recipe = parsed.recipe;
  assert(recipe);
  const status = qualificationReadiness(track, level, recipe);
  assert.equal(status.scope.recipe.id, 'ecommerce.sequential-l1');
  assert.match(status.scope.calibration.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(status.launch.ok, true);
  assert.equal(status.requiredEvidence.length, 7);
  assert.equal(status.qualification.ready, false);
  assert(status.commands.every(command => command.includes('--recipe ecommerce.sequential-l1')));
  const defaultStatus = qualificationReadiness('ecommerce', 1);
  assert.equal(defaultStatus.scope.recipe.id, 'ecommerce.sequential-l1');
  assert(defaultStatus.commands.every(command =>
    command.includes('--recipe ecommerce.sequential-l1')));
});

test('pending modular L2 resolves only the current exact recipe', () => {
  assert.equal(qualificationReadiness('ecommerce', 2,
    'ecommerce.sequential-l2').scope.recipe.id, 'ecommerce.sequential-l2');
  assert.equal(qualificationReadiness('ecommerce', 2).scope.recipe.id, 'ecommerce.sequential-l2');
});
