import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { applyAgentCredential } from '../src/agents/agent-credentials.js';
import type { AgentCredentialArgs } from '../src/agents/agent-credentials.js';

const paid = { id: 'paid', apiKeyEnvironmentVariable: 'PROVIDER_API_KEY' };
const modelFree = { id: 'model-free', apiKeyEnvironmentVariable: null };

test('credential resolution reads only the selected adapter secret file', () => {
  for (const { args, env, path } of [
    { args: {}, env: { PROVIDER_API_KEY_FILE: '/selected/key', ANTHROPIC_API_KEY_FILE: '/unrelated/key' },
      path: '/selected/key' },
    { args: {}, env: { STACK_BENCH_API_KEY_FILE: 'relative-key' }, path: 'relative-key' },
  ] as { args: AgentCredentialArgs; env: Record<string, string>; path: string }[]) {
    let readPath: string | null = null;
    applyAgentCredential(args, paid, { env, read: file => { readPath = file; return 'selected-secret\n'; } });
    assert.equal(readPath, resolve(path));
    assert.equal(args.apiKey, 'selected-secret');
    assert.equal(args.apiKeyFile, resolve(path));
  }
});

test('model-free adapters ignore unrelated provider secrets but reject explicit credentials', () => {
  const args: AgentCredentialArgs = {};
  applyAgentCredential(args, modelFree,
    { env: { ANTHROPIC_API_KEY_FILE: '/mounted/by-appliance' } });
  assert.deepEqual(args, {});
  assert.throws(() => applyAgentCredential({}, modelFree,
    { env: { STACK_BENCH_API_KEY_FILE: '/generic/key' }, read: () => 'secret' }),
  /does not accept an API key/);
});

test('credential resolution rejects empty selected credentials', () => {
  assert.throws(() => applyAgentCredential({}, paid,
    { env: { PROVIDER_API_KEY_FILE: '/key' }, read: () => '\n' }), /is empty/);
});
