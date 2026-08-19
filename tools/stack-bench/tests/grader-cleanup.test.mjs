import assert from 'node:assert/strict';
import test from 'node:test';

import { closeActorContexts } from '../grader/grade.mjs';

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
    { context, name: 'buyer', page: { video: () => video } },
  ], { trace: true, media: '/tmp/media', slug: 'account-create' });

  assert.deepEqual(failures.map(failure => failure.stage),
    ['trace', 'context-close', 'video-save', 'video-delete']);
  assert(failures.every(failure => failure.actor === 'buyer'));
});

test('grader context cleanup stays silent when cleanup succeeds', async () => {
  const context = { tracing: { stop: async () => {} }, close: async () => {} };
  const failures = await closeActorContexts([
    { context, name: 'buyer', page: { video: () => null } },
  ], { trace: true, media: '/tmp/media', slug: 'account-create' });
  assert.deepEqual(failures, []);
});
