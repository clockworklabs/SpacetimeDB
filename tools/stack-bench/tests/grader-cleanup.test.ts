import assert from 'node:assert/strict';
import test from 'node:test';
import type { Browser } from 'playwright';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

import { closeActorContexts, gradeFeature } from '../grader/grade.js';
import { harnessBrowserFailure,
  runBrowserInfrastructureOperation } from '../src/evidence/harness-errors.js';

test('grader context cleanup records browser failures instead of throwing away the report', async () => {
  const context = {
    tracing: { stop: async () => { throw new Error('trace target closed'); } },
    close: async () => { throw new Error('browser context closed unexpectedly'); },
  };
  const video = {
    saveAs: async () => { throw new Error('video unavailable'); },
    delete: async () => { throw new Error('video already removed'); },
  };
  const failures = await closeActorContexts([
    { context, name: 'buyer', page: { video: () => video }, traceStarted: true },
  ], { trace: true, media: '/tmp/media', slug: 'account-create' });

  assert.deepEqual(failures.map(failure => failure.stage),
    ['trace', 'context-close', 'video-save', 'video-delete']);
  assert(failures.every(failure => failure.actor === 'buyer'));
});

test('grader context cleanup stays silent when cleanup succeeds', async () => {
  const context = { tracing: { stop: async () => { throw new Error('trace was never started'); } }, close: async () => {} };
  const failures = await closeActorContexts([
    { context, name: 'buyer', page: { video: () => null } },
  ], { trace: true, media: '/tmp/media', slug: 'account-create' });
  assert.deepEqual(failures, []);
});

test('grader context cleanup closes a context when page creation failed', async () => {
  let closed = false;
  const context = {
    tracing: { stop: async () => {} },
    close: async () => { closed = true; },
  };
  const failures = await closeActorContexts([
    { context, name: 'buyer', page: null },
  ], { media: '/tmp/media', slug: 'account-create' });
  assert.equal(closed, true);
  assert.deepEqual(failures, []);
});

test('browser setup operations are harness failures but app navigation is not', async () => {
  let infrastructure: unknown;
  try {
    await runBrowserInfrastructureOperation('page creation', async () => {
      throw new Error('page allocation failed');
    });
  } catch (error) { infrastructure = error; }
  assert.match(harnessBrowserFailure(infrastructure) ?? '', /browser page creation failed/);
  assert.equal(harnessBrowserFailure(new Error('net::ERR_CONNECTION_REFUSED')), null);
});


test('a cancelled browser session cannot start later feature work or media collection', async () => {
  const calls: string[] = [];
  const browser = new Proxy({} as Browser, { get(_target, property) {
    calls.push(String(property));
    throw new Error('cancelled browser must not be accessed');
  } });
  const scenario = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
    name: 'cancelled feature', features: [{ id: 1, name: 'after cancellation', actors: ['buyer'],
      setup: [{ do: 'signUp', actor: 'buyer', name: 'buyer' }],
      criteria: [{ id: '1a', desc: 'account exists', points: 1,
        steps: [{ do: 'expect', actor: 'buyer', testid: 'current-user' }] }] }] }, { source: 'cancelled.json' });
  const result = await gradeFeature(browser, scenario.features[0]!, {
    level: 1, headed: false, selectedCheckKeys: [], nullControl: false,
    media: '/unused-media', failureMedia: '/unused-failure-media', trace: true,
  }, { actionCancellation: { reason: 'previous action cleanup failed' }, runId: 'cancel-test',
    roomName: name => name, url: 'http://unused', actions: [], spacetime: null, nullControl: false });
  assert.deepEqual(calls, []);
  assert.equal(result.score, 0);
  assert.equal(result.setupEvidence.status, 'harness_failure');
  assert(result.criteria.every(criterion => criterion.evidence.status === 'harness_failure'));
  assert.equal(result.cleanupEvidence?.status, 'harness_failure');
});
