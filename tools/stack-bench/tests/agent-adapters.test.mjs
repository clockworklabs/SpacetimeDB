import assert from 'node:assert/strict';
import test from 'node:test';

import { AGENT_ADAPTER_SCHEMA_VERSION, agentRequestArgv, createAgentAdapterRegistry,
  agentSessionFailure, defineAgentAdapter, validateAgentResult } from '../agent-adapter-contract.mjs';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../agent-adapters.mjs';

const request = { mode: 'build', level: 1, app: 'C:\\bench\\app', backend: 'stub',
  track: 'loop', runIndex: 0, model: 'deterministic', guidance: 'prescribed', skills: null };

test('built-in agent adapters are statically registered and content identified', () => {
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.ids,
    ['claude-code', 'deterministic', 'fault-injection', 'reference-fixture']);
  for (const id of AGENT_ADAPTER_REGISTRY.ids) {
    const identity = agentAdapterIdentity(AGENT_ADAPTER_REGISTRY.get(id));
    assert.equal(identity.id, id);
    assert.equal(identity.version, id === 'claude-code' ? '1.5.0' : '1.0.0');
    assert.match(identity.sha256, /^[a-f0-9]{64}$/);
  }
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.get('claude-code').requiredExecutables, ['claude']);
  assert.equal(AGENT_ADAPTER_REGISTRY.get('claude-code').usesStackSkills, true);
  const statusCommand = AGENT_ADAPTER_REGISTRY.get('claude-code').credentialStatusCommand;
  assert.equal(statusCommand[0], 'node');
  assert.match(statusCommand.at(-1), /loggedIn===true/);
  assert.match(statusCommand.at(-1), /authMethod==='claude\.ai'/);
});

test('requests are normalized and unsupported modes fail before launch', () => {
  const deterministic = AGENT_ADAPTER_REGISTRY.get('deterministic');
  assert.deepEqual(agentRequestArgv(deterministic, request).slice(1, 7),
    ['--mode', 'build', '--backend', 'stub', '--level', '1']);
  assert.deepEqual(agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('claude-code'),
    { ...request, maxBudgetUsd: 12.5 }).slice(-2),
    ['--max-budget-usd', '12.5']);
  const guidanceDocument = { path: 'backends/stub.md', sha256: 'a'.repeat(64), bytes: 12 };
  const withDocument = agentRequestArgv(deterministic, { ...request, guidanceDocument });
  assert.equal(withDocument[withDocument.indexOf('--guidance-document-json') + 1],
    JSON.stringify(guidanceDocument));
  assert.equal(agentRequestArgv(deterministic, { ...request, maxBudgetUsd: 12.5 })
    .includes('--max-budget-usd'), false);
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

test('an unsuccessful provider session is a harness failure even when it has an id', () => {
  assert.equal(agentSessionFailure({ ok: true, sessionId: 'session-1' }), null);
  assert.deepEqual(agentSessionFailure({ ok: false, sessionId: 'session-2',
    providerMetadata: { failureCode: 'provider-session-error' } }), {
    kind: 'harness_failure', phase: 'coding-session', reason: 'provider-session-error',
    appFailures: [], inconclusive: [], harnessFailures: [],
  });
  assert.equal(agentSessionFailure({ ok: true, sessionId: null }).reason, 'coding session did not run');
});

test('malformed and duplicate agent adapters fail at registry construction', () => {
  const source = { schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION, id: 'fake', version: '1.0.0',
    entrypoint: 'fake.mjs', modes: ['build'], deadlineMs: 1000,
    defaultModel: 'fake-model', apiKeyEnvironmentVariable: null,
    credentialFiles: [], outboundDestinations: [], requiredExecutables: [],
    credentialStatusCommand: null, usesStackSkills: false,
    costLimit: 'unsupported' };
  assert.equal(defineAgentAdapter(source).id, 'fake');
  assert.throws(() => createAgentAdapterRegistry([source, source]), /duplicate/);
  assert.throws(() => defineAgentAdapter({ ...source, version: 'latest' }), /version/);
  assert.throws(() => defineAgentAdapter({ ...source, modes: ['unknown'] }), /modes/);
  assert.throws(() => defineAgentAdapter({ ...source, credentialFiles: ['..\\secret'] }), /credentialFiles/);
  assert.throws(() => defineAgentAdapter({ ...source, outboundDestinations: ['http://insecure.example'] }),
    /outboundDestinations/);
  assert.throws(() => defineAgentAdapter({ ...source, requiredExecutables: ['../claude'] }),
    /requiredExecutables/);
  assert.throws(() => defineAgentAdapter({ ...source, credentialStatusCommand: ['claude', 'bad\narg'] }),
    /credentialStatusCommand/);
  assert.throws(() => defineAgentAdapter({ ...source, command: 'node' }), /command is unknown/);
});
