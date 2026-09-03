import assert from 'node:assert/strict';
import test from 'node:test';

import { GRADING_CAPABILITY_IDS } from '../src/actions/action-contract.js';
import { describeStackTestabilityProblems,
  resolveStackTestability } from '../src/campaigns/stack-testability.js';
import type { CompiledRecipePlan } from '../src/composition/composition-compiler.js';
import type { CompiledStep } from '../src/composition/definition-compiler.js';
import type { TrackAction } from '../src/composition/tracks.js';

const boundAction = { id: 'buy', path: '/api/buy', reducer: 'buy_now', args: [0],
  params: [{ name: 'itemId', in: 'body' }] };

const criteria: Record<string, CompiledStep[]> = {
  replay: [{ do: 'replayAs', actor: 'other', from: 'owner', match: 'secret' }],
  namedReplay: [{ do: 'replayAs', actor: 'other', from: 'owner', match: 'secret',
    namedAction: boundAction }],
  routeOnly: [{ do: 'replayAs', actor: 'other', from: 'owner', match: 'secret',
    namedAction: { id: 'ship', path: '/api/ship', args: [] } }],
  trackCall: [{ do: 'callAction', actor: 'owner', action: 'buy', input: {} }],
  unknownCall: [{ do: 'callAction', actor: 'owner', action: 'refund', input: {} }],
  stock: [{ do: 'dbSetStock', item: 'Lamp', warehouse: 'East', quantity: 1 }],
  raced: [{ do: 'race', branches: [[{ do: 'forgeWrite', actor: 'a', fromActor: 'b' }],
    [{ do: 'click', actor: 'a', testid: 'buy-now' }]] }],
  browser: [{ do: 'click', actor: 'owner', testid: 'buy-now' }],
};

function plan(setup: CompiledStep[] = []): CompiledRecipePlan {
  const feature = { id: 1, name: 'feature', setup,
    criteria: Object.entries(criteria).map(([id, steps]) => ({ id, desc: id, points: 1, steps })) };
  return {
    checks: Object.keys(criteria).map(id => ({ stableKey: `pack.feature.${id}`, packId: 'pack',
      checkGroupId: 'group', criterionId: id, role: 'feature', source: 'scenario.json',
      featureId: 1, description: id, sourcePoints: 1, points: 1 })),
    execution: [{ id: 'exec', source: 'scenario.json', checkGroups: [{ packId: 'pack',
      packVersion: '1.0.0', checkGroupId: 'group', role: 'feature', source: 'scenario.json',
      feature, actions: [] }] }],
  } as unknown as CompiledRecipePlan;
}

const http = { id: 'postgres', grading: { transport: 'http' as const, capabilities: GRADING_CAPABILITY_IDS } };
const reducer = { id: 'spacetime', grading: { transport: 'reducer' as const, capabilities: GRADING_CAPABILITY_IDS } };
const stub = { id: 'stub', grading: { transport: 'http' as const,
  capabilities: GRADING_CAPABILITY_IDS.filter(id => id !== 'database-write') } };
const trackActions = [boundAction] as unknown as TrackAction[];
const keys = (...ids: string[]) => ids.map(id => `pack.feature.${id}`);

const resolve = (checkKeys: string[], stacks = [http, reducer, stub], setup?: CompiledStep[]) =>
  describeStackTestabilityProblems(resolveStackTestability({
    plan: plan(setup), checkKeys, trackActions, stacks }));

test('a replay without a named action only resolves where HTTP writes are captured', () => {
  assert.deepEqual(resolve(keys('replay')), [
    'spacetime cannot measure pack.feature.replay: replayAs re-issues a captured HTTP write, '
    + 'and spacetime issues writes as reducer calls; give the step a named action',
  ]);
  assert.deepEqual(resolve(keys('namedReplay')), []);
});

test('named actions need the binding each transport issues', () => {
  assert.deepEqual(resolve(keys('routeOnly')), [
    'spacetime cannot measure pack.feature.routeOnly: replayAs application action ship declares no reducer for spacetime',
  ]);
  assert.deepEqual(resolve(keys('trackCall')), []);
  assert.deepEqual(resolve(keys('unknownCall'), [http]), [
    'postgres cannot measure pack.feature.unknownCall: callAction names no application action "refund"',
  ]);
});

test('runtime capabilities come from the action registry and the stack declaration', () => {
  assert.deepEqual(resolve(keys('stock')), [
    'stub cannot measure pack.feature.stock: dbSetStock needs the database-write capability, which stub does not provide',
  ]);
  assert.deepEqual(resolve(keys('browser')), []);
});

test('branches and feature setup count, unselected criteria do not', () => {
  assert.deepEqual(resolve(keys('raced'), [reducer]), [
    'spacetime cannot measure pack.feature.raced: forgeWrite re-issues a captured HTTP write, '
    + 'and spacetime issues writes as reducer calls; give the step a named action',
  ]);
  assert.deepEqual(resolve(keys('browser'), [stub], criteria.stock), [
    'stub cannot measure pack.feature.browser: dbSetStock needs the database-write capability, which stub does not provide',
  ]);
  assert.deepEqual(resolve(keys('browser'), [reducer]), []);
  assert.throws(() => resolve(keys('missing')), /selected check pack\.feature\.missing is not in the recipe/);
});
