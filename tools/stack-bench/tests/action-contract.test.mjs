import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { ACTION_REGISTRY, legacyActionPlugin } from '../action-catalog.mjs';
import {
  ACTION_EVIDENCE_SCHEMA_VERSION,
  ACTION_INPUT_SCHEMA_VERSION,
  ACTION_PLUGIN_SCHEMA_VERSION,
  ActionApplicationFailure,
  ActionInconclusive,
  createActionRegistry,
  createActionRunContext,
  executeAction,
} from '../action-contract.mjs';
import { ACTION_IDS } from '../definition-compiler.mjs';

const fixture = JSON.parse(readFileSync(
  join(import.meta.dirname, 'fixtures', 'definitions', 'all-actions.json'), 'utf8'));

function steps(value) {
  const out = [];
  const visit = step => {
    out.push(step);
    if (step.do === 'race') step.branches.flat().forEach(visit);
  };
  for (const feature of value.features) {
    [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)].forEach(visit);
  }
  return out;
}

function plugin(overrides = {}) {
  return {
    schemaVersion: ACTION_PLUGIN_SCHEMA_VERSION,
    id: 'fakeAction', version: '1.0.0',
    input: { schemaVersion: ACTION_INPUT_SCHEMA_VERSION,
      compile: input => {
        if (input?.do !== 'fakeAction') throw new Error('wrong fake input');
        return structuredClone(input);
      } },
    capabilities: ['clock'],
    deadline: { timeoutMs: 250 },
    evidence: { schemaVersion: ACTION_EVIDENCE_SCHEMA_VERSION, type: 'fake-evidence',
      validate: value => value?.ok === true },
    redaction: { sensitivity: ['public'], fields: [] },
    renderer: { label: 'fake action', category: 'test' },
    execute: ({ input, capabilities, signal, implementation, attempt }) =>
      implementation({ input, capabilities, signal, attempt }),
    ...overrides,
  };
}

function context(implementation, overrides = {}) {
  return createActionRunContext({ capabilities: { clock: {}, hidden: 'must not leak' },
    implementations: { fakeAction: implementation }, attempt: { id: 'attempt-a' }, ...overrides });
}

test('all 47 compatibility actions have complete versioned runtime contracts', () => {
  assert.deepEqual(ACTION_REGISTRY.ids, ACTION_IDS);
  const representative = new Map(steps(fixture).map(step => [step.do, step]));
  for (const id of ACTION_IDS) {
    const action = ACTION_REGISTRY.get(id);
    assert.equal(action.schemaVersion, ACTION_PLUGIN_SCHEMA_VERSION);
    assert.match(action.version, /^\d+\.\d+\.\d+$/);
    assert(action.capabilities.length > 0, id);
    assert(action.deadline.timeoutMs > 0, id);
    assert.match(action.evidence.type, /-evidence$/, id);
    assert(action.redaction.sensitivity.every(value => typeof value === 'string'), id);
    assert.doesNotThrow(() => action.input.compile(representative.get(id)), id);
  }
});

test('duplicate, unknown, malformed, and incomplete registrations fail at startup', () => {
  assert.throws(() => createActionRegistry([plugin(), plugin()]), /duplicate action registration/);
  assert.throws(() => createActionRegistry([plugin()], { expectedIds: ['fakeAction', 'missing'] }),
    /missing missing/);
  assert.throws(() => createActionRegistry([plugin(), { ...plugin(), id: 'surprise' }],
    { expectedIds: ['fakeAction'] }), /unknown surprise/);
  assert.throws(() => ACTION_REGISTRY.get('notReal'), /unknown registered action/);
  assert.throws(() => createActionRegistry([{ ...plugin(), deadline: { timeoutMs: 0 } }]),
    /positive integer/);
  assert.throws(() => createActionRegistry([{ ...plugin(), mystery: true }]), /mystery is unknown/);
  assert.throws(() => legacyActionPlugin('notReal'), /unknown compatibility action/);
});

test('a successful action sees only declared capabilities and returns structured evidence', async () => {
  const registry = createActionRegistry([plugin()]);
  const result = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    context(({ capabilities, attempt }) => {
      assert.deepEqual(Object.keys(capabilities), ['clock']);
      assert.equal(attempt.id, 'attempt-a');
      return { ok: true, value: 7 };
    }));
  assert.equal(result.status, 'passed');
  assert.equal(result.code, 'completed');
  assert.deepEqual(result.observation, { ok: true, value: 7 });
  assert.equal(result.action.id, 'fakeAction');
  assert.equal(result.timing.deadlineMs, 250);
});

test('application failure and inconclusive results stay distinct', async () => {
  const registry = createActionRegistry([plugin()]);
  const failed = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    context(() => { throw new ActionApplicationFailure('subject rejected assertion',
      { observation: { actual: 2 }, expected: { value: 1 } }); }));
  assert.equal(failed.status, 'failed');
  assert.deepEqual(failed.observation, { actual: 2 });
  assert.deepEqual(failed.expected, { value: 1 });
  const inconclusive = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    context(() => { throw new ActionInconclusive('transport cannot be observed'); }));
  assert.equal(inconclusive.status, 'inconclusive');
  assert.equal(inconclusive.code, 'inconclusive');
});

test('deadline, cancellation, and unclassified exceptions are fail-closed evidence', async () => {
  const quick = createActionRegistry([plugin({ deadline: { timeoutMs: 20 } })]);
  const never = ({ signal }) => new Promise((_resolve, reject) =>
    signal.addEventListener('abort', () => reject(signal.reason), { once: true }));
  const timedOut = await executeAction(quick, 'fakeAction', { do: 'fakeAction' }, context(never));
  assert.equal(timedOut.status, 'harness_failure');
  assert.equal(timedOut.code, 'deadline_exceeded');
  assert.equal(timedOut.retryable, true);

  const controller = new AbortController();
  const cancelledPromise = executeAction(createActionRegistry([plugin()]), 'fakeAction',
    { do: 'fakeAction' }, context(never, { signal: controller.signal }));
  controller.abort('operator cancelled');
  const cancelled = await cancelledPromise;
  assert.equal(cancelled.status, 'inconclusive');
  assert.equal(cancelled.code, 'cancelled');

  const crashed = await executeAction(createActionRegistry([plugin()]), 'fakeAction',
    { do: 'fakeAction' }, context(() => { throw new TypeError('unexpected bug'); }));
  assert.equal(crashed.status, 'harness_failure');
  assert.equal(crashed.code, 'unclassified_exception');
});

test('invalid input, missing services, and malformed observations never become passes', async () => {
  const registry = createActionRegistry([plugin()]);
  const invalid = await executeAction(registry, 'fakeAction', { do: 'wrong' }, context(() => ({ ok: true })));
  assert.equal(invalid.code, 'invalid_input');
  const missingCapability = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    createActionRunContext({ capabilities: {}, implementations: { fakeAction: () => ({ ok: true }) } }));
  assert.equal(missingCapability.code, 'missing_capability');
  const missingImplementation = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    createActionRunContext({ capabilities: { clock: {} }, implementations: {} }));
  assert.equal(missingImplementation.code, 'missing_implementation');
  const malformed = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    context(() => ({ ok: false })));
  assert.equal(malformed.code, 'invalid_evidence');
  assert.equal(malformed.status, 'harness_failure');
  const cyclic = { ok: true };
  cyclic.self = cyclic;
  const nonSerializable = await executeAction(registry, 'fakeAction', { do: 'fakeAction' },
    context(() => cyclic));
  assert.equal(nonSerializable.code, 'invalid_evidence');
  assert.throws(() => createActionRunContext({ capabilities: {}, implementations: {}, surprise: true }),
    /surprise is unknown/);
  assert.throws(() => createActionRunContext({ capabilities: {}, implementations: {},
    attempt: { id: 'a', token: 'must-not-leak' } }), /token is unknown/);
});
