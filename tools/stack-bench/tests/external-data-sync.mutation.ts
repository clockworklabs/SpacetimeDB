import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature }
  from '../src/composition/definition-compiler.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';

const ROOT = STACK_BENCH_ROOT;
const LIVE_SCENARIO = 'tracks/ecommerce/scenarios/01-external-live-sync.json';
const RELOAD_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reload-sync.json';
const RECONNECT_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reconnect-sync.json';

function json(relative: string): unknown {
  return JSON.parse(readFileSync(join(ROOT, relative), 'utf8'));
}

test('external synchronization scenarios are focused and state-independent', () => {
  const live = compileScenarioDefinition(json(LIVE_SCENARIO), {
    source: LIVE_SCENARIO,
    expectedLevel: 1,
  });
  const reconnect = compileScenarioDefinition(json(RECONNECT_SCENARIO), {
    source: RECONNECT_SCENARIO,
    expectedLevel: 1,
  });
  const reload = compileScenarioDefinition(json(RELOAD_SCENARIO), {
    source: RELOAD_SCENARIO,
    expectedLevel: 1,
  });

  const liveFeature = requiredFeature(live.features[0], LIVE_SCENARIO);
  const reconnectFeature = requiredFeature(reconnect.features[0], RECONNECT_SCENARIO);
  const reloadFeature = requiredFeature(reload.features[0], RELOAD_SCENARIO);
  const liveCriterion = requiredCriterion(liveFeature.criteria[0], LIVE_SCENARIO);
  const reconnectCriterion = requiredCriterion(reconnectFeature.criteria[0], RECONNECT_SCENARIO);
  const reloadCriterion = requiredCriterion(reloadFeature.criteria[0], RELOAD_SCENARIO);

  assert.deepEqual(liveFeature.criteria.map(criterion => criterion.id), ['901a']);
  assert.deepEqual(liveCriterion.steps.map(step => step.do),
    ['dbSetStock', 'expectNumber']);
  assert.equal(liveCriterion.points, 1,
    'the scenario and recipe score must agree');
  assert.deepEqual(reloadCriterion.steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
  assert.equal(reloadCriterion.points, 0,
    'reload persistence is supporting evidence, not a second score');

  const reconnectSteps = reconnectCriterion.steps;
  assert.deepEqual(reconnectSteps.map(step => step.do),
    ['setOffline', 'dbSetStock', 'setOffline', 'expectNumber']);
  const disconnectedWrite = reconnectSteps[1];
  const reconnectResult = reconnectSteps.at(-1);
  assert(disconnectedWrite && reconnectResult, 'reconnect checks must contain their boundary steps');
  assert.equal(disconnectedWrite.settleMs, 4000,
    'the external write must remain inside the disconnected window before network restoration');
  assert.equal(reconnectResult.equals, 52,
    'East 7 + untouched West 45 must not depend on another external-stock scenario');
  assert.equal(reconnectSteps.some(step => ['startAppServer', 'stopAppServer'].includes(step.do)), false);
  assert.equal(reconnectCriterion.points, 1,
    'the scenario and recipe score must agree');
});

test('901b is independently selectable from the current L1 recipe', () => {
  const release = buildRecipeRelease(join(ROOT, 'tracks', 'ecommerce', 'composition', 'recipes',
    'sequential-l1.json'));
  const key = 'ecommerce.spec.external-data-sync.external-stock.901b';
  const check = release.checkCatalog.find(candidate => candidate.stableKey === key);
  assert(check, `${key} must exist in the release`);
  assert.equal(check.source, 'scenarios/01-external-reload-sync.json');
  assert.equal(check.points, 0);
  const reloadScenario = compileScenarioDefinition(json(RELOAD_SCENARIO), {
    source: RELOAD_SCENARIO,
    expectedLevel: 1,
  });
  const selected = selectScenarioChecks(reloadScenario, { checks: release.checkCatalog }, [key]);
  const feature = requiredFeature(selected.features[0], RELOAD_SCENARIO);
  const criterion = requiredCriterion(feature.criteria[0], RELOAD_SCENARIO);
  const setup = feature.setup[0];
  assert(setup, 'the selected reload scenario must have setup');
  assert.deepEqual(feature.criteria.map(item => item.id), ['901b']);
  assert.equal(setup.equals, 100);
  assert.deepEqual(criterion.steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
});

function requiredFeature(feature: CompiledFeature | undefined, source: string): CompiledFeature {
  if (!feature) throw new Error(`${source} must contain a feature`);
  return feature;
}

function requiredCriterion(
  criterion: CompiledCriterion | undefined,
  source: string,
): CompiledCriterion {
  if (!criterion) throw new Error(`${source} must contain a criterion`);
  return criterion;
}
