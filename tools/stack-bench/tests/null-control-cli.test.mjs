import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { compileScenarioDefinition } from '../definition-compiler.mjs';
import { nullControlSuites, parseNullControlArgs } from '../null-control.mjs';
import { resolveRecipeRelease } from '../recipe-release.mjs';
import { selectScenarioChecks } from '../recipe-selection.mjs';
import { loadTrack } from '../tracks.mjs';

test('null qualification can select one exact track and level', () => {
  const args = parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '1']);
  assert.deepEqual(args.tracks, ['ecommerce']);
  assert.equal(args.level, 1);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce,chat', '--level', '1']), /exactly one/);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '0']), /positive integer/);
  assert.equal(parseNullControlArgs(['node', 'null-control.mjs', '--track', 'ecommerce',
    '--level', '1', '--recipe', 'ecommerce.l1-standard@1.1.0']).recipe,
  'ecommerce.l1-standard@1.1.0');
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs', '--track', 'ecommerce',
    '--recipe', 'ecommerce.l1-standard@1.1.0']), /requires --level/);
  const track = loadTrack('ecommerce');
  assert.deepEqual(nullControlSuites(track, 2).map(suite => suite.level), [2, 2, 2, 2, 2]);
  assert.throws(() => nullControlSuites(track, 4), /not declared/);
});

test('recipe-bound null qualification grades the exact modular execution and checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.3.0');
  const suites = nullControlSuites(track, 1, binding);
  const checks = suites.flatMap(suite => suite.checks);

  assert.deepEqual(suites.map(suite => suite.id), binding.execution.map(execution => execution.id));
  assert.equal(checks.length, 48);
  assert.equal(checks.reduce((total, check) => total + check.points, 0), 58);
  assert.equal(checks.filter(check => check.points === 0).length, 2);
  assert.equal(new Set(checks.map(check => check.stableKey)).size, 48);

  const selectedByExecution = new Map(suites.map(suite => {
    const scenario = compileScenarioDefinition(JSON.parse(readFileSync(suite.spec, 'utf8')),
      { source: suite.spec, expectedLevel: 1 });
    const selected = selectScenarioChecks(scenario, { checks: binding.release.checkCatalog },
      suite.checks.map(check => check.stableKey));
    const selectedKeys = selected.features.flatMap(feature => feature.criteria
      .map(criterion => suite.checks.find(check => check.featureId === feature.id
        && check.criterionId === criterion.id)?.stableKey)).filter(Boolean).sort();
    assert.deepEqual(selectedKeys, suite.checks.map(check => check.stableKey).sort());
    return [suite.id, selectedKeys];
  }));
  const serverActions = [...selectedByExecution.entries()]
    .find(([id]) => id.includes('server-actions'))?.[1] ?? [];
  const invariants = [...selectedByExecution.entries()]
    .find(([id]) => id.includes('invariants'))?.[1] ?? [];
  for (const criterion of ['101a', '102a', '103a', '104a']) {
    assert(serverActions.some(key => key.endsWith(`.${criterion}`)));
    assert.equal(invariants.some(key => key.endsWith(`.${criterion}`)), false);
  }
});

test('recipe-bound null qualification rejects an execution with no mapped checks', () => {
  const track = loadTrack('ecommerce');
  const binding = structuredClone(resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.3.0'));
  binding.execution.push({ id: 'empty-execution', source: binding.execution[0].source,
    ownership: { kind: 'current', level: 1 } });
  assert.throws(() => nullControlSuites(track, 1, binding), /maps no checks/);
});
