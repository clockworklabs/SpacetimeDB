import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, compileRecipeFile, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledFeature, type CompiledStep }
  from '../src/composition/definition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const candidateRecipe = recipePacks(readJson(join(trackRoot, 'composition', 'recipes', 'sequential-l3-1.0.0.json')));
const packNames = candidateRecipe
  .map(pack => pack.path.split('/').at(-1))
  .filter((name): name is string => name !== undefined && name.startsWith('l3-'))
  .sort();
const packs = packNames.map(name => compilePackDefinition(readJson(join(packRoot, name)), { source: name }));
const packById = new Map(candidateRecipe.map(pack => {
  const name = pack.path.split('/').at(-1) ?? '';
  const compiled = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
  return [compiled.id, compiled];
}));
const selected = packs.flatMap(pack => pack.checks.map(check => ({ pack, check })));
const scenarioFor = (check: CompiledPackDefinition['checks'][number]) => {
  const source = join(trackRoot, check.source);
  return compileScenarioDefinition(readJson(source), { source, expectedLevel: 3 });
};
const featureFor = (check: CompiledPackDefinition['checks'][number]): CompiledFeature => {
  const feature = scenarioFor(check).features.find(candidate => candidate.id === check.feature);
  if (!feature) throw new Error(`${check.id} must select a feature`);
  return feature;
};
const nestedSteps = (feature: CompiledFeature): CompiledStep[] =>
  [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)].flatMap(flatten);

function flatten(step: CompiledStep): CompiledStep[] {
  if (step.do !== 'race') return [step];
  if (!Array.isArray(step.branches)) throw new Error('race step must have branches');
  return [step, ...step.branches.flatMap(branch => branch.flatMap(flatten))];
}

function requiredPack(id: string): CompiledPackDefinition {
  const pack = packs.find(candidate => candidate.id === id);
  if (!pack) throw new Error(`missing pack ${id}`);
  return pack;
}

function requiredCheck(pack: CompiledPackDefinition, id?: string): CompiledPackDefinition['checks'][number] {
  const check = id === undefined ? pack.checks[0] : pack.checks.find(candidate => candidate.id === id);
  if (!check) throw new Error(`missing check ${id ?? 'first'} in ${pack.id}`);
  return check;
}

function firstCriterion(feature: CompiledFeature) {
  const criterion = feature.criteria[0];
  if (!criterion) throw new Error(`feature ${feature.id} must have a criterion`);
  return criterion;
}

function lastStep(steps: CompiledStep[]): CompiledStep {
  const step = steps.at(-1);
  if (!step) throw new Error('scenario must have at least one step');
  return step;
}

function atLeast(value: unknown, minimum: number): boolean {
  return typeof value === 'number' && value >= minimum;
}

function atMost(value: unknown, maximum: number): boolean {
  return typeof value === 'number' && value <= maximum;
}

function maxRuntimeMs(pack: CompiledPackDefinition): number {
  const runtime = pack.budget.maxRuntimeMs;
  if (typeof runtime !== 'number') throw new Error(`${pack.id} must declare maxRuntimeMs`);
  return runtime;
}

function resolveFragment(fragment: CompiledPackDefinition['task']['requirements'][number]): string {
  const path = join(trackRoot, fragment.path);
  assert(existsSync(path), `missing fragment source ${fragment.path}`);
  const text = readFileSync(path, 'utf8');
  const start = fragment.from ? text.indexOf(fragment.from) : 0;
  assert(start >= 0, `${fragment.id} has no start marker ${fragment.from}`);
  const end = fragment.until ? text.indexOf(fragment.until, start + (fragment.from?.length ?? 0)) : text.length;
  assert(end > start, `${fragment.id} resolves to empty or reversed text`);
  return text.slice(start, end).trim();
}

test('L3 product work and production specifications are separate modules', () => {
  assert.equal(packs.length, 8);
  assert.equal(packs.filter(pack => pack.moduleType === 'feature').length, 4);
  assert.equal(packs.filter(pack => pack.moduleType === 'specification').length, 4);

  for (const pack of packs) {
    const promptPaths = pack.task.requirements.map(fragment => fragment.path);
    if (pack.moduleType === 'feature') {
      assert(promptPaths.every(path => path === 'prompts/modular/l3-features-1.0.0.md'));
      assert(pack.checks.every(check => check.role === 'feature'));
    } else {
      assert(promptPaths.every(path => path === 'prompts/modular/l3-specifications-1.0.0.md'));
      assert(pack.task.requirements.every(fragment => fragment.requiresFeatures?.length));
      assert(pack.checks.every(check => check.role === 'guarantee'));
      assert(pack.checks.every(check => check.observations?.includes('unmentioned')));
    }
  }
});

test('every L3 scored check has isolated setup and one criterion', () => {
  assert.equal(selected.length, 22);
  const stableKeys = new Set();

  for (const { pack, check } of selected) {
    const feature = featureFor(check);
    assert(feature, `${pack.id}.${check.id} must select a real feature`);
    assert.equal(feature.criteria.length, 1,
      `${pack.id}.${check.id} must not depend on another criterion's side effects`);
    assert(feature.setup.length > 0, `${pack.id}.${check.id} must own its setup`);
    const criterion = firstCriterion(feature);
    const key = `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`;
    assert(!stableKeys.has(key), `duplicate stable check ${key}`);
    stableKeys.add(key);
  }
});

test('L3 dependencies close over real exact pack releases', () => {
  const visit = (pack: CompiledPackDefinition, seen = new Set<string>()): Set<string> => {
    const ref = `${pack.id}@${pack.version}`;
    if (seen.has(ref)) return seen;
    seen.add(ref);
    for (const required of pack.requiresPacks) {
      const dependency = packById.get(required);
      assert(dependency, `${ref} requires missing ${required}`);
      visit(dependency, seen);
    }
    return seen;
  };
  for (const pack of packs) visit(pack);
  const order = requiredPack('ecommerce.l3.order-delivery-features');
  assert(visit(order).has('ecommerce.returns-pricing-features@1.1.0'));
});

test('L3 prompt and contract fragments resolve to non-empty text', () => {
  const fragments = new Map();
  for (const pack of packs) {
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      const text = resolveFragment(fragment);
      assert(text.length > 20, `${fragment.id} is too small to be useful`);
      const previous = fragments.get(fragment.id);
      if (previous) {
        assert.equal(text, previous, `${fragment.id} must resolve identically for every owner`);
      } else {
        fragments.set(fragment.id, text);
      }
    }
  }
  assert.equal(
    packs.flatMap(pack => pack.task.contracts)
      .filter(fragment => fragment.id === 'ecommerce.l3.reservation-hooks').length,
    2,
    'reservations and cart expiration must share one testing-interface fragment',
  );
  const scheduled = requiredPack('ecommerce.l3.scheduled-restocks-features');
  const calls = scheduled.task.contracts.find(fragment =>
    fragment.id === 'ecommerce.l3.scheduled-restock-testing-calls');
  assert(calls, 'scheduled restocks must include direct testing calls');
  assert.match(resolveFragment(calls), /DELETE \/api\/admin\/scheduled-restocks\/:id/);
  assert.match(resolveFragment(calls), /cancelScheduledRestock/);
});

test('L3 feature requests and production specifications do not claim the same work', () => {
  const featureText = readFileSync(join(trackRoot, 'prompts', 'modular', 'l3-features-1.0.0.md'), 'utf8');
  const specificationText = readFileSync(
    join(trackRoot, 'prompts', 'modular', 'l3-specifications-1.0.0.md'), 'utf8');
  assert.doesNotMatch(featureText, /survive a backend restart|more than once|without an open browser|Only an admin/);
  assert.doesNotMatch(specificationText, /shows the remaining|enters the stock ledger|returns empty/);

  const featureCriteria = selected.filter(entry => entry.pack.moduleType === 'feature')
    .map(entry => firstCriterion(featureFor(entry.check)));
  const specificationCriteria = selected.filter(entry => entry.pack.moduleType === 'specification')
    .map(entry => firstCriterion(featureFor(entry.check)));
  const featureSteps = new Set(featureCriteria.map(criterion => JSON.stringify(criterion.steps)));
  assert(specificationCriteria.every(criterion => !featureSteps.has(JSON.stringify(criterion.steps))),
    'a production check must not duplicate a product check byte for byte');
});

test('the sequential L3 brief agrees with cumulative L2 shipping behavior', () => {
  const sequentialBrief = readFileSync(join(trackRoot, 'prompts', '03-scheduled.md'), 'utf8');
  assert.match(sequentialBrief, /Shipping remains the immediate staff action introduced at level 2/);
  assert.match(sequentialBrief, /shipped order moves to `delivered` \*\*60 seconds\*\* after shipping/);
  assert.doesNotMatch(sequentialBrief, /pending.*moves to.*shipped/is);
  assert.doesNotMatch(sequentialBrief, /time it actually ran/);
});

test('L3 capability and evidence declarations match the selected actions', () => {
  const knownCapabilities = new Set([
    'backend-lifecycle', 'browser', 'concurrency', 'concurrent-actors', 'database-observation',
    'direct-database-write', 'direct-server-call', 'request-replay',
  ]);
  const knownEvidence = new Set([
    'browser-observation', 'concurrent-outcome', 'database-observation', 'fresh-client-observation',
    'server-action-observation', 'server-refusal', 'server-response',
  ]);
  for (const pack of packs) {
    assert(pack.capabilities.every(value => knownCapabilities.has(value)));
    assert(pack.evidence.every(value => knownEvidence.has(value)));
    const actions = selected.filter(entry => entry.pack.id === pack.id)
      .flatMap(entry => nestedSteps(featureFor(entry.check)).map(step => step.do));
    if (actions.includes('restartBackend')) assert(pack.capabilities.includes('backend-lifecycle'));
    if (actions.includes('callAction')) {
      assert(pack.capabilities.includes('direct-server-call'));
      assert(pack.evidence.includes('server-response'));
    }
    if (actions.includes('replayAs')) {
      assert(pack.capabilities.includes('request-replay'));
      assert(pack.evidence.includes('server-refusal'));
    }
    assert(!pack.evidence.includes('fresh-client-observation'),
      `${pack.id} does not run freshClient and must not claim fresh-client evidence`);
  }
});

test('timed and restarted work has conclusive before-and-after observations', () => {
  const durationText = readFileSync(join(trackRoot, 'prompts', 'modular', 'l3-features-1.0.0.md'), 'utf8');
  assert.match(durationText, /90 seconds/);

  const durability = requiredPack('ecommerce.l3.deferred-durability-specifications');
  for (const check of durability.checks) {
    const feature = featureFor(check);
    const steps = nestedSteps(feature);
    const restart = steps.findIndex(step => step.do === 'restartBackend');
    assert(restart > 0);
    assert(steps.slice(0, restart).some(step => ['expect', 'expectNumber'].includes(step.do)),
      `${check.id} needs a pre-restart observation`);
    assert(steps.slice(restart + 1).some(step => ['expect', 'expectNumber'].includes(step.do)),
      `${check.id} needs a post-restart completion observation`);
  }

  const timedChecks: ReadonlyArray<readonly [packId: string, checkId: string]> = [
    ['ecommerce.l3.reservations-features', 'countdown'],
    ['ecommerce.l3.scheduled-restocks-features', 'pending'],
  ];
  for (const [packId, checkId] of timedChecks) {
    const pack = requiredPack(packId);
    const feature = featureFor(requiredCheck(pack, checkId));
    const numbers = nestedSteps(feature).filter(step => step.do === 'expectNumber');
    assert(numbers.some(step => atLeast(step.atLeast, 80)));
    assert(numbers.some(step => atMost(step.atMost, 75)));
    assert(nestedSteps(feature).some(step => step.do === 'wait' && atLeast(step.ms, 15000)));
  }
});

test('scheduled-work access tests both server actions and cleans up pending work', () => {
  const pack = requiredPack('ecommerce.l3.deferred-access-specifications');
  const steps = nestedSteps(featureFor(requiredCheck(pack)));
  assert(steps.some(step => step.do === 'callAction' && step.action === 'scheduleRestock'));
  assert(steps.some(step => step.do === 'replayAs'
    && step.namedAction?.id === 'cancelScheduledRestock'));
  assert(steps.some(step => step.do === 'expectActionOutcome' && step.outcome === 'refused'));
  assert(steps.some(step => step.do === 'expectReplayRejected'));
  assert.equal(lastStep(steps).do, 'click');
  assert.equal(lastStep(steps).testid, 'pending-restock-cancel');
});

test('pending-work checks cancel or complete the work they create', () => {
  const pendingPack = requiredPack('ecommerce.l3.scheduled-restocks-features');
  const pendingSteps = nestedSteps(featureFor(
    requiredCheck(pendingPack, 'pending')));
  assert.equal(lastStep(pendingSteps).do, 'click');
  assert.equal(lastStep(pendingSteps).testid, 'pending-restock-cancel');

  const timePack = requiredPack('ecommerce.l3.server-time-specifications');
  const timeSteps = nestedSteps(featureFor(requiredCheck(timePack, 'not-early')));
  assert(timeSteps.some(step => step.do === 'expectNumber' && step.plus === 4));
  assert.equal(timeSteps.some(step => step.testid === 'pending-restock-remaining'), false);
  assert.equal(lastStep(timeSteps).do, 'expect');
  assert.equal(lastStep(timeSteps).absent, true);
});

test('L3 budgets cover declared waits without becoming unbounded estimates', () => {
  for (const pack of packs) {
    const declaredDelay = selected.filter(entry => entry.pack.id === pack.id)
      .reduce((total, entry) => {
        const steps = nestedSteps(featureFor(entry.check));
        const fixed = steps.reduce((sum, step) =>
          sum + Number(step.ms ?? 0) + Number(step.settleMs ?? 0), 0);
        const longestObservation = Math.max(0, ...steps.map(step => Number(step.within ?? 0)));
        return total + fixed + longestObservation;
      }, 0);
    assert(maxRuntimeMs(pack) >= declaredDelay,
      `${pack.id} budget is below its declared waits`);
    assert(maxRuntimeMs(pack) <= declaredDelay + 180000,
      `${pack.id} budget has more than three minutes of unexplained slack`);
  }
});

test('L3 covers each declared product area and production behavior', () => {
  assert.deepEqual(new Set(packs.filter(pack => pack.moduleType === 'feature').map(pack => pack.id)), new Set([
    'ecommerce.l3.reservations-features',
    'ecommerce.l3.scheduled-restocks-features',
    'ecommerce.l3.order-delivery-features',
    'ecommerce.l3.cart-expiration-features',
  ]));
  assert.deepEqual(new Set(packs.filter(pack => pack.moduleType === 'specification').map(pack => pack.id)), new Set([
    'ecommerce.l3.deferred-durability-specifications',
    'ecommerce.l3.deferred-integrity-specifications',
    'ecommerce.l3.server-time-specifications',
    'ecommerce.l3.deferred-access-specifications',
  ]));
});

test('the cumulative L3 recipe adds every L3 check', () => {
  const plan = compileRecipeFile(
    join(trackRoot, 'composition', 'recipes', 'sequential-l3-1.0.0.json'),
    { trackRoot },
  );
  assert.equal(plan.checks.length, 98);
  assert.equal(plan.scoring.points, 180);
  assert.equal(plan.execution.length, 49);

  const plannedKeys = new Set(plan.checks.map(check => check.stableKey));
  const expectedL3Keys = selected.flatMap(({ pack, check }) => {
    const feature = featureFor(check);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion => {
      if (!criterion) throw new Error(`${check.id} must select a real criterion`);
      return `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`;
    });
  });
  assert.equal(expectedL3Keys.length, 22);
  assert(expectedL3Keys.every(key => plannedKeys.has(key)));

  const reservationHooks = plan.recipe.task.contracts.find(fragment =>
    fragment.id === 'ecommerce.l3.reservation-hooks');
  assert(reservationHooks, 'L3 recipe must contain reservation hooks');
  assert.deepEqual(reservationHooks.owners, [
    'ecommerce.l3.cart-expiration-features',
    'ecommerce.l3.reservations-features',
  ]);
});

function recipePacks(value: unknown): Array<{ path: string }> {
  if (!isRecord(value) || !Array.isArray(value.packs)) throw new Error('candidate recipe must have packs');
  return value.packs.map((pack, index) => {
    if (!isRecord(pack) || typeof pack.path !== 'string') {
      throw new Error(`candidate recipe pack ${index} must have a path`);
    }
    return { path: pack.path };
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
