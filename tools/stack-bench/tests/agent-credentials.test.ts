import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { applyAgentCredential } from '../src/agents/agent-credentials.js';
import type { AgentCredentialArgs } from '../src/agents/agent-credentials.js';

const paid = { id: 'paid', apiKeyEnvironmentVariable: 'PROVIDER_API_KEY' };
const modelFree = { id: 'model-free', apiKeyEnvironmentVariable: null };

test('credential resolution reads only the selected adapter secret file', () => {
  let readPath: string | null = null;
  const args: AgentCredentialArgs = {};
  applyAgentCredential(args, paid, { env: {
    PROVIDER_API_KEY_FILE: '/selected/key',
    ANTHROPIC_API_KEY_FILE: '/unrelated/key',
  }, read: path => { readPath = path; return 'selected-secret\n'; } });
  assert.equal(readPath, resolve('/selected/key'));
  assert.equal(args.apiKey, 'selected-secret');
});

test('credential resolution normalizes explicit paths', () => {
  const input: AgentCredentialArgs = { apiKeyFile: 'relative-key' };
  applyAgentCredential(input, paid, { env: {}, read: () => 'secret' });
  assert.equal(input.apiKeyFile, resolve('relative-key'));
  assert.equal(input.apiKey, 'secret');
});

test('model-free adapters ignore unrelated provider secrets but reject explicit credentials', () => {
  const args: AgentCredentialArgs = {};
  applyAgentCredential(args, modelFree,
    { env: { ANTHROPIC_API_KEY_FILE: '/mounted/by-appliance' } });
  assert.deepEqual(args, {});
  assert.throws(() => applyAgentCredential({ apiKeyFile: '/explicit/key' }, modelFree,
    { env: {}, read: () => 'secret' }), /does not accept an API key/);
  assert.throws(() => applyAgentCredential({}, modelFree,
    { env: { STACK_BENCH_API_KEY_FILE: '/generic/key' }, read: () => 'secret' }),
  /does not accept an API key/);
});

test('credential resolution rejects ambiguous and empty selected credentials', () => {
  assert.throws(() => applyAgentCredential({ apiKey: 'direct', apiKeyFile: '/key' }, paid,
    { env: {}, read: () => 'file' }), /only one/);
  assert.throws(() => applyAgentCredential({ apiKeyFile: '/key' }, paid,
    { env: {}, read: () => '\n' }), /is empty/);
});
