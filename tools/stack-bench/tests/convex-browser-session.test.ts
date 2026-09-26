import assert from 'node:assert/strict';
import test from 'node:test';
import { recordConvexSession } from '../src/stacks/backends/convex-browser-session.js';
import { bindBrowserRequest } from '../src/actions/named-action-runtime.js';
import type { Actor } from '../src/actions/actor-action-runtime.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';

test('Convex replay binds the observed caller session, including tampering, without guessing field names', () => {
  const actor = { name: 'caller', page: {}, writes: [] } as unknown as Actor;
  const request = { url: 'http://localhost:3210/api/mutation', responseContract: 'convex-mutation' as const,
    body: JSON.stringify({ path: 'api:buy', args: { itemId: 'item', quantity: 1 }, format: 'json' }) };
  const headers = { Authorization: 'Bearer current-session' };
  const record = (frame: unknown, port = 3210) => recordConvexSession(actor.page,
    `ws://localhost:${port}/api/1.0.0/sync`, JSON.stringify(frame));
  for (const frame of [null, false, 12, 'not a native frame', []]) assert.doesNotThrow(() => record(frame));
  assert.throws(() => bindBrowserRequest(actor, request, headers), ActionInconclusive);
  record({ type: 'Mutation', args: [{ customSession: 'current-session' }] }, 3211);
  assert.throws(() => bindBrowserRequest(actor, request, headers), ActionInconclusive);
  record({ type: 'ModifyQuerySet', modifications: [{ type: 'Add', args: [{ customSession: 'current-session' }] }] });
  const bind = bindBrowserRequest(actor, request, headers);
  assert.deepEqual(bind(), { headers: {}, body: JSON.stringify({ path: 'api:buy',
    args: { itemId: 'item', quantity: 1, customSession: 'current-session' }, format: 'json' }) });
  assert.equal(JSON.parse(bind({ Authorization: 'Bearer corrupted-session' }).body!).args.customSession, 'corrupted-session');
  assert.equal(JSON.parse(request.body).args.customSession, undefined);
  assert.throws(() => bindBrowserRequest(actor, request, { Authorization: 'Bearer rotated-session' }), ActionInconclusive);
  record({ type: 'Authenticate', tokenType: 'User', value: 'native-session' });
  record({ type: 'Mutation', args: [{ queryOnlyToken: 'native-session' }] });
  assert.deepEqual(bindBrowserRequest(actor, request, { Authorization: 'Bearer native-session' })(),
    { headers: { Authorization: 'Bearer native-session' }, body: request.body });
  record({ type: 'Mutation', args: [{ anotherField: 'current-session' }] });
  assert.throws(() => bindBrowserRequest(actor, request, headers), ActionInconclusive);
  for (let n = 0; n < 201; n++) record({ type: 'Mutation', args: [{ queryOnlyToken: 'native-session' }] });
  assert.deepEqual(bindBrowserRequest(actor, request, { Authorization: 'Bearer native-session' })(),
    { headers: { Authorization: 'Bearer native-session' }, body: request.body });
});

test('Convex replay accepts native sessions through the explicit application proxy only', () => {
  const actor = { name: 'caller', page: {}, writes: [] } as unknown as Actor;
  const request = { url: 'http://localhost:3210/api/mutation', applicationOrigin: 'http://localhost:6923',
    responseContract: 'convex-mutation' as const,
    body: JSON.stringify({ path: 'api:buy', args: { itemId: 'item' }, format: 'json' }) };
  const headers = { Authorization: 'Bearer current-session' };
  const query = JSON.stringify({ type: 'ModifyQuerySet', modifications: [
    { type: 'Add', args: [{ token: 'current-session' }] },
  ] });
  recordConvexSession(actor.page, 'ws://unrelated.test/api/1.0.0/sync', query);
  assert.throws(() => bindBrowserRequest(actor, request, headers), ActionInconclusive);
  recordConvexSession(actor.page, 'ws://localhost:6923/api/1.0.0/sync', query);
  assert.throws(() => bindBrowserRequest(actor, { ...request, applicationOrigin: undefined }, headers), ActionInconclusive);
  const replay = bindBrowserRequest(actor, request, headers)();
  assert.deepEqual(replay.headers, {});
  assert.equal(JSON.parse(replay.body!).args.token, 'current-session');
  recordConvexSession(actor.page, 'ws://localhost:6923/api/1.0.0/sync',
    JSON.stringify({ type: 'Authenticate', tokenType: 'User', value: 'current-session' }));
  assert.deepEqual(bindBrowserRequest(actor, request, headers)(), { headers, body: request.body });
});
