import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const packNames = readdirSync(packRoot).filter(name => name.startsWith('l3-') && name.endsWith('.json')).sort();
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const packs = packNames.map(name => compilePackDefinition(readJson(join(packRoot, name)), { source: name }));
const allPacks = readdirSync(packRoot).filter(name => name.endsWith('.json'))
  .map(name => compilePackDefinition(readJson(join(packRoot, name)), { source: name }));
const packByRef = new Map(allPacks.map(pack => [`${pack.id}@${pack.version}`, pack]));
const selected = packs.flatMap(pack => pack.checks.map(check => ({ pack, check })));
const scenarioFor = check => {
  const source = join(trackRoot, check.source);
  return compileScenarioDefinition(readJson(source), { source, expectedLevel: 3 });
};
const featureFor = check => scenarioFor(check).features.find(feature => feature.id === check.feature);
const nestedSteps = feature => [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)]
  .flatMap(function flatten(step) {
    return step.do === 'race' ? [step, ...step.branches.flatMap(branch => branch.flatMap(flatten))] : [step];
  });

function resolveFragment(fragment) {
  const path = join(trackRoot, fragment.path);
  assert(existsSync(path), `missing fragment source ${fragment.path}`);
  const text = readFileSync(path, 'utf8');
  const start = fragment.from ? text.indexOf(fragment.from) : 0;
  assert(start >= 0, `${fragment.id} has no start marker ${fragment.from}`);
  const end = fragment.until ? text.indexOf(fragment.until, start + (fragment.from?.length ?? 0)) : text.length;
  assert(end > start, `${fragment.id} resolves to empty or reversed text`);
  return text.slice(start, end).trim();
}

test('L3 product work and production specifications are separate draft modules', () => {
  assert.equal(packs.length, 8);
  assert(packs.every(pack => pack.state === 'draft'));
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
      assert(pack.checks.every(check => check.observations.includes('unmentioned')));
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
    const criterion = feature.criteria[0];
    const key = `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`;
    assert(!stableKeys.has(key), `duplicate stable check ${key}`);
    stableKeys.add(key);
  }
});

test('L3 dependencies close over real exact pack releases', () => {
  const visit = (pack, seen = new Set()) => {
    const ref = `${pack.id}@${pack.version}`;
    if (seen.has(ref)) return seen;
    seen.add(ref);
    for (const required of pack.requiresPacks) {
      const dependency = packByRef.get(required);
      assert(dependency, `${ref} requires missing ${required}`);
      visit(dependency, seen);
    }
    return seen;
  };
  for (const pack of packs) visit(pack);
  const order = packs.find(pack => pack.id === 'ecommerce.l3.order-delivery-features');
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
  const scheduled = packs.find(pack => pack.id === 'ecommerce.l3.scheduled-restocks-features');
  const calls = scheduled.task.contracts.find(fragment =>
    fragment.id === 'ecommerce.l3.scheduled-restock-testing-calls');
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
    .map(entry => featureFor(entry.check).criteria[0]);
  const specificationCriteria = selected.filter(entry => entry.pack.moduleType === 'specification')
    .map(entry => featureFor(entry.check).criteria[0]);
  const featureSteps = new Set(featureCriteria.map(criterion => JSON.stringify(criterion.steps)));
  assert(specificationCriteria.every(criterion => !featureSteps.has(JSON.stringify(criterion.steps))),
    'a production check must not duplicate a product check byte for byte');
});

test('the legacy L3 brief agrees with cumulative L2 shipping behavior', () => {
  const legacy = readFileSync(join(trackRoot, 'prompts', '03-scheduled.md'), 'utf8');
  assert.match(legacy, /Shipping remains the immediate staff action introduced at level 2/);
  assert.match(legacy, /shipped order moves to `delivered` \*\*60 seconds\*\* after shipping/);
  assert.doesNotMatch(legacy, /pending.*moves to.*shipped/is);
  assert.doesNotMatch(legacy, /time it actually ran/);
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

  const durability = packs.find(pack => pack.id === 'ecommerce.l3.deferred-durability-specifications');
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

  for (const [packId, checkId] of [
    ['ecommerce.l3.reservations-features', 'countdown'],
    ['ecommerce.l3.scheduled-restocks-features', 'pending'],
  ]) {
    const pack = packs.find(candidate => candidate.id === packId);
    const feature = featureFor(pack.checks.find(check => check.id === checkId));
    const numbers = nestedSteps(feature).filter(step => step.do === 'expectNumber');
    assert(numbers.some(step => step.atLeast >= 80));
    assert(numbers.some(step => step.atMost <= 75));
    assert(nestedSteps(feature).some(step => step.do === 'wait' && step.ms >= 15000));
  }
});

test('scheduled-work access tests both server actions and cleans up pending work', () => {
  const pack = packs.find(candidate => candidate.id === 'ecommerce.l3.deferred-access-specifications');
  const steps = nestedSteps(featureFor(pack.checks[0]));
  assert(steps.some(step => step.do === 'callAction' && step.action === 'scheduleRestock'));
  assert(steps.some(step => step.do === 'replayAs'
    && step.namedAction?.id === 'cancelScheduledRestock'));
  assert(steps.some(step => step.do === 'expectActionOutcome' && step.outcome === 'refused'));
  assert(steps.some(step => step.do === 'expectReplayRejected'));
  assert.equal(steps.at(-1).do, 'click');
  assert.equal(steps.at(-1).testid, 'pending-restock-cancel');
});

test('pending-work checks cancel or complete the work they create', () => {
  const pendingPack = packs.find(candidate =>
    candidate.id === 'ecommerce.l3.scheduled-restocks-features');
  const pendingSteps = nestedSteps(featureFor(
    pendingPack.checks.find(check => check.id === 'pending')));
  assert.equal(pendingSteps.at(-1).do, 'click');
  assert.equal(pendingSteps.at(-1).testid, 'pending-restock-cancel');

  const timePack = packs.find(candidate =>
    candidate.id === 'ecommerce.l3.server-time-specifications');
  const timeSteps = nestedSteps(featureFor(timePack.checks.find(check => check.id === 'not-early')));
  assert(timeSteps.some(step => step.do === 'expectNumber' && step.plus === 4));
  assert.equal(timeSteps.some(step => step.testid === 'pending-restock-remaining'), false);
  assert.equal(timeSteps.at(-1).do, 'expect');
  assert.equal(timeSteps.at(-1).absent, true);
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
    assert(pack.budget.maxRuntimeMs >= declaredDelay,
      `${pack.id} budget is below its declared waits`);
    assert(pack.budget.maxRuntimeMs <= declaredDelay + 180000,
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

test('the L3 candidate carries the promoted L2 release and adds every L3 check', () => {
  const binding = resolveRecipeRelease(
    loadTrack('ecommerce'),
    3,
    'ecommerce.l3-standard@1.0.0',
  );
  assert.equal(binding.status, 'candidate');
  assert.equal(binding.plan.checks.length, 98);
  assert.equal(binding.plan.scoring.points, 180);
  assert.equal(binding.plan.execution.length, 49);

  const current = binding.execution.filter(entry => entry.ownership.kind === 'current');
  const inherited = binding.execution.filter(entry => entry.ownership.kind === 'inherited');
  assert.equal(current.length, 8);
  assert.equal(inherited.length, 41);

  const plannedKeys = new Set(binding.plan.checks.map(check => check.stableKey));
  const expectedL3Keys = selected.flatMap(({ pack, check }) => {
    const feature = featureFor(check);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion =>
      `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`);
  });
  assert.equal(expectedL3Keys.length, 22);
  assert(expectedL3Keys.every(key => plannedKeys.has(key)));

  const reservationHooks = binding.plan.recipe.task.contracts.find(fragment =>
    fragment.id === 'ecommerce.l3.reservation-hooks');
  assert.deepEqual(reservationHooks.owners, [
    'ecommerce.l3.cart-expiration-features',
    'ecommerce.l3.reservations-features',
  ]);
});
