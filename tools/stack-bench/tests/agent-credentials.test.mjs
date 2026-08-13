import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { resolveAgentCredential } from '../agent-credentials.mjs';

const paid = { id: 'paid', apiKeyEnvironmentVariable: 'PROVIDER_API_KEY' };
const modelFree = { id: 'model-free', apiKeyEnvironmentVariable: null };

test('credential resolution reads only the selected adapter secret file', () => {
  let readPath = null;
  const args = resolveAgentCredential({}, paid, { env: {
    PROVIDER_API_KEY_FILE: '/selected/key',
    ANTHROPIC_API_KEY_FILE: '/unrelated/key',
  }, read: path => { readPath = path; return 'selected-secret\n'; } });
  assert.equal(readPath, resolve('/selected/key'));
  assert.equal(args.apiKey, 'selected-secret');
});

test('model-free adapters ignore unrelated provider secrets but reject explicit credentials', () => {
  assert.deepEqual(resolveAgentCredential({}, modelFree,
    { env: { ANTHROPIC_API_KEY_FILE: '/mounted/by-appliance' } }), {});
  assert.throws(() => resolveAgentCredential({ apiKeyFile: '/explicit/key' }, modelFree,
    { env: {}, read: () => 'secret' }), /does not accept an API key/);
  assert.throws(() => resolveAgentCredential({}, modelFree,
    { env: { STACK_BENCH_API_KEY_FILE: '/generic/key' }, read: () => 'secret' }),
  /does not accept an API key/);
});

test('credential resolution rejects ambiguous and empty selected credentials', () => {
  assert.throws(() => resolveAgentCredential({ apiKey: 'direct', apiKeyFile: '/key' }, paid,
    { env: {}, read: () => 'file' }), /only one/);
  assert.throws(() => resolveAgentCredential({ apiKeyFile: '/key' }, paid,
    { env: {}, read: () => '\n' }), /is empty/);
});
