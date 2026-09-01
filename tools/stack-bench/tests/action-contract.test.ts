import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { ACTION_REGISTRY, actionPlugin } from '../src/actions/action-catalog.js';
import {
  ActionApplicationFailure,
  ActionInconclusive,
  createActionRegistry,
  executeAction,
} from '../src/actions/action-contract.js';
import type {
  ActionImplementation,
  ActionPlugin,
} from '../src/actions/action-contract.js';
import { ACTION_IDS } from '../src/composition/definition-compiler.js';

interface FixtureStep {
  do: string;
  branches?: FixtureStep[][];
}

interface FixtureCriterion {
  steps: FixtureStep[];
}

interface FixtureFeature {
  setup: FixtureStep[];
  criteria: FixtureCriterion[];
}

interface FixtureDefinition {
  features: FixtureFeature[];
}

const fixture = JSON.parse(readFileSync(
  join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'definitions', 'all-actions.json'),
  'utf8')) as FixtureDefinition;

function steps(value: FixtureDefinition): FixtureStep[] {
  const out: FixtureStep[] = [];
  const visit = (step: FixtureStep): void => {
    out.push(step);
    if (step.do === 'race') (step.branches ?? []).flat().forEach(visit);
  };
  for (const feature of value.features) {
    [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)].forEach(visit);
  }
  return out;
}

function plugin(overrides: Partial<ActionPlugin> = {}): ActionPlugin {
  return {
    id: 'fakeAction', version: '1.0.0',
    category: 'transport',
    compile: (input: unknown) => {
      if (typeof input !== 'object' || input === null || !('do' in input)
        || input.do !== 'fakeAction') throw new Error('wrong fake input');
      return structuredClone(input);
    },
    capabilities: ['clock'],
    timeoutMs: 250,
    sensitivity: ['public'],
    execute: () => ({ ok: true }),
    ...overrides,
  };
}

function context(overrides: Record<string, unknown> = {}) {
  return { capabilities: { clock: {}, hidden: 'must not leak' }, ...overrides };
}

function registry(implementation: ActionImplementation, overrides: Partial<ActionPlugin> = {}) {
  return createActionRegistry([plugin({ ...overrides, execute: implementation })]);
}

test('every scenario action has a complete versioned runtime contract', () => {
  assert.deepEqual(ACTION_REGISTRY.ids, ACTION_IDS);
  const representative = new Map(steps(fixture).map(step => [step.do, step]));
  for (const id of ACTION_IDS) {
    const action = ACTION_REGISTRY.get(id);
    assert.match(action.version, /^\d+\.\d+\.\d+$/);
    assert(action.capabilities.length > 0, id);
    assert(action.timeoutMs > 0, id);
    assert(action.sensitivity.every(value => typeof value === 'string'), id);
    assert.doesNotThrow(() => action.compile(representative.get(id)), id);
  }
});

test('action policy is explicit for each behavior category', () => {
  const expected = {
    callAction: ['transport', 60_000, ['actors', 'named-actions', 'transport-observation']],
    callConcurrently: ['concurrency', 120_000, ['actors', 'named-actions']],
    click: ['browser-interaction', 60_000, ['actors', 'browser-interaction']],
    dbSetStock: ['database', 90_000, ['clock', 'database-write']],
    expect: ['browser-observation', 300_000, ['actors', 'browser-observation']],
    restartBackend: ['lifecycle', 900_000, ['backend-lifecycle']],
    runScript: ['application-process', 90_000, ['application-files', 'subprocess']],
    startAppServer: ['lifecycle', 900_000, ['application-lifecycle']],
    wait: ['timing', 360_000, ['actors', 'clock']],
  } as const;
  for (const [id, [category, timeoutMs, capabilities]] of Object.entries(expected)) {
    const action = ACTION_REGISTRY.get(id);
    assert.equal(action.category, category, id);
    assert.equal(action.timeoutMs, timeoutMs, id);
    assert.deepEqual(action.capabilities, capabilities, id);
  }
  assert.deepEqual(ACTION_REGISTRY.get('runScript').sensitivity,
    ['user-content', 'filesystem-path', 'process-output']);
  assert.deepEqual(ACTION_REGISTRY.get('signIn').sensitivity,
    ['credential', 'user-content']);
});

test('duplicate, unknown, malformed, and incomplete registrations fail at startup', () => {
  assert.throws(() => createActionRegistry([plugin(), plugin()]), /duplicate action registration/);
  assert.throws(() => createActionRegistry([plugin()], { expectedIds: ['fakeAction', 'missing'] }),
    /missing missing/);
  assert.throws(() => createActionRegistry([plugin(), { ...plugin(), id: 'surprise' }],
    { expectedIds: ['fakeAction'] }), /unknown surprise/);
  assert.throws(() => ACTION_REGISTRY.get('notReal'), /unknown registered action/);
  assert.throws(() => actionPlugin('notReal'), /unknown action/);
});

test('a successful action sees only declared capabilities and returns structured evidence', async () => {
  const result = await executeAction(registry(({ capabilities }) => {
      assert.deepEqual(Object.keys(capabilities), ['clock']);
      return { ok: true, value: 7 };
    }), 'fakeAction', { do: 'fakeAction' }, context());
  assert.equal(result.status, 'passed');
  assert.equal(result.code, 'completed');
  assert.deepEqual(result.observation, { ok: true, value: 7 });
  assert.equal(result.action.id, 'fakeAction');
  assert.equal(result.timing.deadlineMs, 250);
});

test('action evidence remains coherent if an injected wall clock moves backward', async () => {
  const ticks = [100, 95];
  const result = await executeAction(registry(() => ({ ok: true })), 'fakeAction',
    { do: 'fakeAction' }, context(), { now: () => ticks.shift() ?? 0 });
  assert.deepEqual(result.timing, {
    startedAtMs: 100,
    completedAtMs: 100,
    durationMs: 0,
    deadlineMs: 250,
  });
});

test('application failure and inconclusive results stay distinct', async () => {
  const failed = await executeAction(registry(() => {
    throw new ActionApplicationFailure('subject rejected assertion',
      { observation: { actual: 2 }, expected: { value: 1 } });
  }), 'fakeAction', { do: 'fakeAction' }, context());
  assert.equal(failed.status, 'failed');
  assert.deepEqual(failed.observation, { actual: 2 });
  assert.deepEqual(failed.expected, { value: 1 });
  const inconclusive = await executeAction(registry(() => {
    throw new ActionInconclusive('transport cannot be observed');
  }), 'fakeAction', { do: 'fakeAction' }, context());
  assert.equal(inconclusive.status, 'inconclusive');
  assert.equal(inconclusive.code, 'inconclusive');
});

test('deadline, cancellation, and unclassified exceptions are fail-closed evidence', async () => {
  let cleanedUp = false;
  const never: ActionImplementation = ({ signal }) => new Promise((_resolve, reject) =>
    signal.addEventListener('abort', () => setTimeout(() => {
      cleanedUp = true;
      reject(signal.reason);
    }, 5), { once: true }));
  const timedOut = await executeAction(registry(never, { timeoutMs: 20 }), 'fakeAction',
    { do: 'fakeAction' }, context());
  assert.equal(timedOut.status, 'harness_failure');
  assert.equal(timedOut.code, 'deadline_exceeded');
  assert.equal(timedOut.retryable, true);
  assert.equal(cleanedUp, true, 'timeout evidence must wait for implementation cleanup');

  const controller = new AbortController();
  const cancelledPromise = executeAction(registry(never), 'fakeAction',
    { do: 'fakeAction' }, context({ signal: controller.signal }));
  controller.abort('operator cancelled');
  const cancelled = await cancelledPromise;
  assert.equal(cancelled.status, 'inconclusive');
  assert.equal(cancelled.code, 'cancelled');

  const crashed = await executeAction(registry(() => { throw new TypeError('unexpected bug'); }),
    'fakeAction', { do: 'fakeAction' }, context());
  assert.equal(crashed.status, 'harness_failure');
  assert.equal(crashed.code, 'unclassified_exception');
});

test('invalid input, missing services, and malformed observations never become passes', async () => {
  const validRegistry = registry(() => ({ ok: true }));
  const invalid = await executeAction(validRegistry, 'fakeAction', { do: 'wrong' }, context());
  assert.equal(invalid.code, 'invalid_input');
  const missingCapability = await executeAction(validRegistry, 'fakeAction', { do: 'fakeAction' },
    { capabilities: {} });
  assert.equal(missingCapability.code, 'missing_capability');
  assert.equal(missingCapability.status, 'harness_failure');
  const malformed = await executeAction(registry(() => new Date()), 'fakeAction',
    { do: 'fakeAction' }, context());
  assert.equal(malformed.code, 'invalid_evidence');
  assert.equal(malformed.status, 'harness_failure');
  const cyclic: { ok: boolean; self?: unknown } = { ok: true };
  cyclic.self = cyclic;
  const nonSerializable = await executeAction(registry(() => cyclic), 'fakeAction',
    { do: 'fakeAction' }, context());
  assert.equal(nonSerializable.code, 'invalid_evidence');
});
