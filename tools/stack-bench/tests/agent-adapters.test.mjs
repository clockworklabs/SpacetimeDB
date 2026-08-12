import assert from 'node:assert/strict';
import test from 'node:test';

import { AGENT_ADAPTER_SCHEMA_VERSION, agentRequestArgv, createAgentAdapterRegistry,
  defineAgentAdapter, validateAgentResult } from '../agent-adapter-contract.mjs';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../agent-adapters.mjs';

const request = { mode: 'build', level: 1, app: 'C:\\bench\\app', backend: 'stub',
  track: 'loop', runIndex: 0, model: 'deterministic', guidance: 'prescribed', skills: null };

test('built-in agent adapters are statically registered and content identified', () => {
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.ids,
    ['claude-code', 'deterministic', 'fault-injection', 'reference-fixture']);
  for (const id of AGENT_ADAPTER_REGISTRY.ids) {
    const identity = agentAdapterIdentity(AGENT_ADAPTER_REGISTRY.get(id));
    assert.equal(identity.id, id);
    assert.equal(identity.version, '1.0.0');
    assert.match(identity.sha256, /^[a-f0-9]{64}$/);
  }
});

test('requests are normalized and unsupported modes fail before launch', () => {
  const deterministic = AGENT_ADAPTER_REGISTRY.get('deterministic');
  assert.deepEqual(agentRequestArgv(deterministic, request).slice(1, 7),
    ['--mode', 'build', '--backend', 'stub', '--level', '1']);
  const reference = AGENT_ADAPTER_REGISTRY.get('reference-fixture');
  assert.throws(() => agentRequestArgv(reference, { ...request, mode: 'fix' }), /does not support mode fix/);
});

test('completion validation rejects wrong identity and malformed usage', () => {
  const valid = { appDir: request.app, mode: 'build', level: 1, ok: true,
    sessionId: 'session-1', costUsd: 0, tokens: 3, outputTokens: 1, turns: 1,
    promptBytes: 20, durationMs: 10, setup: { isolation: { mode: 'test' } },
    usage: { input: 1, output: 1, cacheWrite: 1, cacheRead: 0 } };
  const normalized = validateAgentResult(valid, request);
  assert.equal(normalized.backend, request.backend);
  assert.equal(normalized.model, request.model);
  assert.deepEqual(normalized.transcript, { kind: 'provider-session', id: 'session-1' });
  assert.throws(() => validateAgentResult({ ...valid, appDir: 'C:\\other' }, request), /appDir/);
  assert.throws(() => validateAgentResult({ ...valid, usage: { ...valid.usage, input: -1 } }, request),
    /usage.input/);
  assert.throws(() => validateAgentResult({ ...valid, sessionId: undefined }, request), /sessionId/);
});

test('malformed and duplicate agent adapters fail at registry construction', () => {
  const source = { schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION, id: 'fake', version: '1.0.0',
    entrypoint: 'fake.mjs', modes: ['build'], deadlineMs: 1000,
    defaultModel: 'fake-model', apiKeyEnvironmentVariable: null,
    credentialFiles: [], outboundDestinations: [] };
  assert.equal(defineAgentAdapter(source).id, 'fake');
  assert.throws(() => createAgentAdapterRegistry([source, source]), /duplicate/);
  assert.throws(() => defineAgentAdapter({ ...source, version: 'latest' }), /version/);
  assert.throws(() => defineAgentAdapter({ ...source, modes: ['unknown'] }), /modes/);
  assert.throws(() => defineAgentAdapter({ ...source, credentialFiles: ['..\\secret'] }), /credentialFiles/);
  assert.throws(() => defineAgentAdapter({ ...source, outboundDestinations: ['http://insecure.example'] }),
    /outboundDestinations/);
  assert.throws(() => defineAgentAdapter({ ...source, command: 'node' }), /command is unknown/);
});
