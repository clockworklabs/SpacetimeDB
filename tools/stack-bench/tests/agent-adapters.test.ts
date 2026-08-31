import assert from 'node:assert/strict';
import test from 'node:test';

import { AGENT_ADAPTER_SCHEMA_VERSION, agentRecipeIdentity, agentRequestArgv, createAgentAdapterRegistry,
  agentSessionFailure, defineAgentAdapter, validateAgentResult }
  from '../src/agents/agent-adapter-contract.js';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity }
  from '../src/agents/agent-adapters.js';
import type { AgentRequest } from '../src/agents/agent-adapter-contract.js';
import type { PricingAuthority } from '../src/evidence/pricing-authority.js';

const request: AgentRequest = { mode: 'build', level: 1, app: 'C:\\bench\\app', backend: 'stub',
  track: 'loop', runIndex: 0, model: 'deterministic', guidance: 'prescribed', skills: null };
const pricing: PricingAuthority = { unit: 'USD-per-million-tokens', rates: {
  input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3,
} };

test('built-in agent adapters are statically registered and content identified', () => {
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.ids,
    ['claude-code', 'deterministic', 'fault-injection', 'reference-fixture']);
  for (const id of AGENT_ADAPTER_REGISTRY.ids) {
    const identity = agentAdapterIdentity(AGENT_ADAPTER_REGISTRY.get(id));
    assert.equal(identity.id, id);
    const expectedVersion = id === 'claude-code' ? '1.17.0'
      : id === 'reference-fixture' ? '1.4.0'
      : id === 'deterministic' ? '1.3.0' : '1.2.0';
    assert.equal(identity.version, expectedVersion);
    assert.match(identity.sha256, /^[a-f0-9]{64}$/);
  }
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.get('claude-code').requiredExecutables, ['claude']);
  assert.equal(AGENT_ADAPTER_REGISTRY.get('claude-code').usesStackSkills, true);
  assert.deepEqual(AGENT_ADAPTER_REGISTRY.get('claude-code').credentialEnvironmentVariables,
    ['CLAUDE_CODE_OAUTH_TOKEN']);
  assert(AGENT_ADAPTER_REGISTRY.get('claude-code').modes.includes('resume'));
  assert(AGENT_ADAPTER_REGISTRY.get('claude-code').deadlineMs
  > AGENT_ADAPTER_REGISTRY.get('deterministic').deadlineMs);
  const statusCommand = AGENT_ADAPTER_REGISTRY.get('claude-code').credentialStatusCommand;
  assert(statusCommand);
  assert.equal(statusCommand[0], 'node');
  const statusScript = statusCommand.at(-1);
  assert(statusScript);
  assert.match(statusScript, /loggedIn===true/);
  assert.match(statusScript, /oauth_token/);
});

test('requests are normalized and unsupported modes fail before launch', () => {
  const deterministic = AGENT_ADAPTER_REGISTRY.get('deterministic');
  assert.deepEqual(agentRequestArgv(deterministic, request).slice(1, 7),
    ['--mode', 'build', '--backend', 'stub', '--level', '1']);
  assert.deepEqual(agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('claude-code'),
    { ...request, maxBudgetUsd: 12.5 }).slice(-2),
    ['--max-budget-usd', '12.5']);
  const priced = agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('claude-code'),
    { ...request, pricing, maxBudgetUsd: 12.5 });
  assert.equal(priced[priced.indexOf('--pricing-json') + 1], JSON.stringify(pricing));
  const guidanceDocument = { path: 'backends/stub.md', sha256: 'a'.repeat(64), bytes: 12 };
  const withDocument = agentRequestArgv(deterministic, { ...request, guidanceDocument });
  assert.equal(withDocument[withDocument.indexOf('--guidance-document-json') + 1],
    JSON.stringify(guidanceDocument));
  const withoutSkills = agentRequestArgv(deterministic, { ...request, skills: [] });
  assert.equal(withoutSkills[withoutSkills.indexOf('--skills-json') + 1], '[]');
  const skillIdentity = { ids: [], sha256: 'a'.repeat(64), bytes: 0 };
  const withSkillIdentity = agentRequestArgv(deterministic,
    { ...request, skills: ['ignored-duplicate'], skillIdentity });
  assert.equal(withSkillIdentity[withSkillIdentity.indexOf('--skill-identity-json') + 1],
    JSON.stringify(skillIdentity));
  assert.equal(withSkillIdentity.includes('--skills-json'), false);
  const recipeTask = { schemaVersion: 1, recipe: {}, selection: {}, task: {} };
  const withRecipeTask = agentRequestArgv(deterministic,
    { ...request, recipe: 'ecommerce.sequential-l1@2.5.0', recipeTask });
  assert.equal(withRecipeTask[withRecipeTask.indexOf('--recipe') + 1],
    'ecommerce.sequential-l1@2.5.0');
  assert.equal(withRecipeTask[withRecipeTask.indexOf('--recipe-task-json') + 1],
    JSON.stringify(recipeTask));
  assert.equal(agentRequestArgv(deterministic, { ...request, maxBudgetUsd: 12.5 })
    .includes('--max-budget-usd'), false);
  const reference = AGENT_ADAPTER_REGISTRY.get('reference-fixture');
  assert.doesNotThrow(() => agentRequestArgv(reference, { ...request, mode: 'upgrade' }));
  assert.doesNotThrow(() => agentRequestArgv(reference, { ...request, mode: 'fix' }));
});

test('a campaign-bound task supplies the exact recipe when no explicit recipe exists', () => {
  const recipeTask = { schemaVersion: 3,
    recipe: { id: 'ecommerce.sequential-l1', version: '2.5.0' },
    selection: {}, task: {} };
  const recipe = agentRecipeIdentity(null, recipeTask);
  assert.equal(recipe, 'ecommerce.sequential-l1@2.5.0');
  const argv = agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('reference-fixture'),
    { ...request, recipe, recipeTask });
  assert.equal(argv[argv.indexOf('--recipe') + 1], 'ecommerce.sequential-l1@2.5.0');
  assert.throws(() => agentRecipeIdentity('ecommerce.sequential-l2@1.6.0', recipeTask),
    /does not match bound task/);
});

test('completion validation rejects wrong identity and malformed usage', () => {
  const nativeRequest: AgentRequest = { ...request,
    adapterCostLimit: 'native', maxBudgetUsd: 1, pricing };
  const receipt = { schemaVersion: 2, source: 'credential-broker', model: request.model,
    maxBudgetUsd: 1, costUsd: 0.0018, cliCostUsd: 0.0018, calculatedCostUsd: 0.0018,
    usage: { input: 100, output: 100, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
    pricingRates: { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6,
      cacheRead: 0.3 },
    complete: true, reconciled: true, error: null };
  const valid = { appDir: request.app, mode: 'build', level: 1, ok: true,
    sessionId: 'session-1', costUsd: 0.0018, tokens: 3, outputTokens: 1, turns: 1,
    promptBytes: 20, durationMs: 10, setup: { isolation: { mode: 'test' } },
    usage: { input: 1, output: 1, cacheWrite: 1, cacheRead: 0 },
    costReceipts: [{ invocation: 1, receipt }] };
  const normalized = validateAgentResult(valid, nativeRequest);
  assert.equal(normalized.backend, request.backend);
  assert.equal(normalized.model, request.model);
  assert.deepEqual(normalized.transcript, { kind: 'provider-session', id: 'session-1' });
  assert.deepEqual(normalized.costReceipts, valid.costReceipts);
  assert.equal(normalized.costComplete, true);
  assert.throws(() => validateAgentResult({ ...valid, appDir: 'C:\\other' }, nativeRequest), /appDir/);
  assert.throws(() => validateAgentResult({ ...valid, usage: { ...valid.usage, input: -1 } }, nativeRequest),
    /usage.input/);
  assert.throws(() => validateAgentResult({ ...valid, sessionId: undefined }, nativeRequest), /sessionId/);
  assert.throws(() => validateAgentResult({ ...valid,
    costReceipts: [{ invocation: 1, receipt: { ...receipt, model: 'wrong' } }] }, nativeRequest),
  /costReceipts/);
  assert.throws(() => validateAgentResult({ ...valid, costReceipts: [] }, nativeRequest),
    /requires complete reconciled broker cost proof/);
  assert.doesNotThrow(() => validateAgentResult({ ...valid, costUsd: 0.00185 }, nativeRequest));
  assert.throws(() => validateAgentResult({ ...valid, costUsd: 0.01 }, nativeRequest),
    /requires complete reconciled broker cost proof/);
  assert.throws(() => validateAgentResult(valid, { ...nativeRequest,
    pricing: { ...pricing, rates: { ...pricing.rates, output: 12 } } }),
  /requires complete reconciled broker cost proof/);
  assert.throws(() => validateAgentResult({ ...valid,
    costReceipts: [{ invocation: 1, receipt: { ...receipt, complete: false,
      reconciled: false, error: 'incomplete' } }] }, nativeRequest),
  /complete reconciled broker cost proof/);
  const failed = validateAgentResult({ ...valid, ok: false,
    costReceipts: [{ invocation: 1, receipt: { ...receipt, complete: false,
      reconciled: false, error: 'incomplete' } }] }, nativeRequest);
  assert.equal(failed.costComplete, false);
  const failedReceipt = failed.costReceipts[0];
  assert(failedReceipt);
  assert.equal(failedReceipt.receipt.error, 'incomplete');
  assert.doesNotThrow(() => validateAgentResult({ ...valid, costUsd: 0, costReceipts: [] },
    { ...request, adapterCostLimit: 'non-billable' }));
  assert.equal(validateAgentResult({ ...valid, costUsd: 0, costReceipts: [] },
    { ...request, adapterCostLimit: 'unsupported' }).costComplete, false);
});

test('provider failures stay separate from harness failures', () => {
  assert.equal(agentSessionFailure({ ok: true, sessionId: 'session-1' }), null);
  assert.deepEqual(agentSessionFailure({ ok: false, sessionId: 'session-2',
    providerMetadata: { failureCode: 'provider-session-error' } }), {
    kind: 'provider_failure', phase: 'coding-session', reason: 'provider-session-error',
    provider: null,
    appFailures: [], inconclusive: [], harnessFailures: [],
  });
  const missingSession = agentSessionFailure({ ok: true, sessionId: null });
  assert(missingSession);
  assert.equal(missingSession.reason, 'coding session did not run');
  const failedSession = agentSessionFailure({ ok: false, sessionId: null,
    providerMetadata: { failureCode: 'coding-session-no-output' } });
  assert(failedSession);
  assert.equal(failedSession.kind, 'harness_failure');
});

test('malformed and duplicate agent adapters fail at registry construction', () => {
  const source = { schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION, id: 'fake', version: '1.0.0',
    entrypoint: 'fake.mjs', modes: ['build'], deadlineMs: 1000,
    defaultModel: 'fake-model', apiKeyEnvironmentVariable: null,
    credentialEnvironmentVariables: [], credentialFiles: [], outboundDestinations: [],
    requiredExecutables: [],
    credentialStatusCommand: null, usesStackSkills: false,
    costLimit: 'unsupported' };
  assert.equal(defineAgentAdapter(source).id, 'fake');
  assert.throws(() => createAgentAdapterRegistry([source, source]), /duplicate/);
  assert.throws(() => defineAgentAdapter({ ...source, version: 'latest' }), /version/);
  assert.throws(() => defineAgentAdapter({ ...source, modes: ['unknown'] }), /modes/);
  assert.throws(() => defineAgentAdapter({ ...source, credentialFiles: ['..\\secret'] }), /credentialFiles/);
  assert.throws(() => defineAgentAdapter({ ...source,
    credentialEnvironmentVariables: ['not-valid'] }), /credentialEnvironmentVariables/);
  assert.throws(() => defineAgentAdapter({ ...source, outboundDestinations: ['http://insecure.example'] }),
    /outboundDestinations/);
  assert.throws(() => defineAgentAdapter({ ...source, requiredExecutables: ['../claude'] }),
    /requiredExecutables/);
  assert.throws(() => defineAgentAdapter({ ...source, credentialStatusCommand: ['claude', 'bad\narg'] }),
    /credentialStatusCommand/);
  assert.throws(() => defineAgentAdapter({ ...source, command: 'node' }), /command is unknown/);
});
