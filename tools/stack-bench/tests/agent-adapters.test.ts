import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { runAgent } from '../commands/bench.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { killTree } from '../src/runtime/platform.js';
import { runBounded } from '../src/runtime/bounded-process.js';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { requireRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';

import { AGENT_ADAPTER_SCHEMA_VERSION, agentRecipeIdentity, agentRequestArgv,
  createAgentAdapterRegistry, defineAgentAdapter }
  from '../src/agents/agent-adapter-contract.js';
import { agentSessionFailure, validateAgentResult }
  from '../src/agents/agent-result-contract.js';
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
    ['claude-code', 'codex', 'deterministic', 'fault-injection', 'openrouter', 'reference-fixture']);
  for (const id of AGENT_ADAPTER_REGISTRY.ids) {
    const identity = agentAdapterIdentity(AGENT_ADAPTER_REGISTRY.get(id));
    assert.equal(identity.id, id);
    const expectedVersion = ['codex', 'openrouter'].includes(id) ? '1.0.0' : id === 'claude-code' ? '1.17.2'
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
  const codex = AGENT_ADAPTER_REGISTRY.get('codex');
  assert.equal(codex.provider, 'openai');
  assert.deepEqual(agentRequestArgv(codex, request).slice(1, 3), ['--provider', 'openai']);
  assert.equal(codex.entrypoint, AGENT_ADAPTER_REGISTRY.get('claude-code').entrypoint);
  const routed = AGENT_ADAPTER_REGISTRY.get('openrouter');
  assert.equal(routed.entrypoint, codex.entrypoint);
  assert.deepEqual(routed.requiredExecutables, codex.requiredExecutables);
  assert.equal(routed.apiKeyEnvironmentVariable, 'OPENROUTER_API_KEY');
  const routedArgs = agentRequestArgv(routed, { ...request, providerRoute: 'openai', maxOutputTokens: 8192 });
  assert.deepEqual(routedArgs.slice(1, 3), ['--provider', 'openrouter']);
  assert.equal(routedArgs[routedArgs.indexOf('--provider-route') + 1], 'openai');
  assert.equal(routedArgs[routedArgs.indexOf('--max-output-tokens') + 1], '8192');
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
    { ...request, recipe: 'ecommerce.sequential-l1', recipeTask });
  assert.equal(withRecipeTask[withRecipeTask.indexOf('--recipe') + 1],
    'ecommerce.sequential-l1');
  assert.equal(withRecipeTask[withRecipeTask.indexOf('--recipe-task-json') + 1],
    JSON.stringify(recipeTask));
  assert.equal(agentRequestArgv(deterministic, { ...request, maxBudgetUsd: 12.5 })
    .includes('--max-budget-usd'), false);
  const reference = AGENT_ADAPTER_REGISTRY.get('reference-fixture');
  assert.doesNotThrow(() => agentRequestArgv(reference, { ...request, mode: 'upgrade' }));
  assert.doesNotThrow(() => agentRequestArgv(reference, { ...request, mode: 'fix' }));
});

test('adapter identity binds grading credentials as well as the executable', () => {
  const adapter = AGENT_ADAPTER_REGISTRY.get('deterministic');
  assert.notEqual(agentAdapterIdentity(adapter).sha256,
    agentAdapterIdentity({ ...adapter, gradesWithFixtureCredentials: true }).sha256);
});

test('a supervisor-owned deadline still requires and obeys cancellation', async () => {
  await assert.rejects(runBounded(process.execPath,[],{ timeoutMs:null,stdio:'ignore' }),/owner abort signal/);
  const owner = new AbortController();
  const timer = setTimeout(() => owner.abort(),100);
  try {
    const result = await runBounded(process.execPath,['-e','setInterval(()=>{},1000)'], {
      timeoutMs:null,signal:owner.signal,stdio:'ignore',
    });
    assert.equal(result.cancelled,true);
    assert.equal(result.timedOut,false);
    assert.equal(result.ok,false);
  } finally { clearTimeout(timer); }
});

test('an external entry point receives exact visible tasks and shares one remaining budget across modes', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-external agent-'));
  try {
    const entrypoint = join(root, 'agent.mjs');
    const received = join(root, 'received.jsonl');
    writeFileSync(entrypoint, `import {appendFileSync} from 'node:fs';
const argv=process.argv.slice(2), args={};
for(let i=0;i<argv.length;i+=2) args[argv[i].slice(2)]=argv[i+1];
appendFileSync(${JSON.stringify(received)},JSON.stringify(args)+'\\n');
console.log(JSON.stringify({appDir:args.app,mode:args.mode,level:Number(args.level),
backend:args.backend,track:args.track,model:args.model,ok:true,sessionId:'external-'+args.mode,
setup:{session:'model-free conformance'},costUsd:0.25,tokens:0,outputTokens:0,
turns:1,promptBytes:0,durationMs:1,usage:{input:0,output:0,cacheWrite:0,cacheRead:0}}));
`);
    const adapter = defineAgentAdapter({ ...AGENT_ADAPTER_REGISTRY.get('deterministic'),
      id: 'external-test', entrypoint });
    const binding = requireRecipeRelease(loadTrack('ecommerce'), 1);
    const selected = createBoundRecipeTaskRequest(binding);
    const visible = createAgentVisibleTaskRequest(binding, selected);
    const args = { ...parseBenchArguments(['node','bench','--backend','postgres','--track','ecommerce',
      '--agent-adapter','deterministic']), model:'external model', maxBudgetUsd:1, spentBudgetUsd:0,
      skills:[], guidanceDocument:{ text:'Line one\n"Quoted" input' },
      recipeTasks:new Map([[1,{ ...selected, agentRequest:visible }]]), recipeBindings:new Map() };
    for (const mode of ['build','upgrade','resume','fix'] as const) {
      const result = await runAgent(args, adapter, mode, 1, root);
      assert.equal(result.ok,true);
      assert.equal(result.sessionId,`external-${mode}`);
    }
    assert.equal(args.spentBudgetUsd,1);
    await assert.rejects(runAgent(args,adapter,'fix',1,root),/exhausted/);
    await assert.rejects(runAgent({ ...args,spentBudgetUsd:0 },
      { ...adapter,costLimit:'unsupported' },'build',1,root),/cannot enforce/);
    const deliveries = readFileSync(received,'utf8').trim().split('\n').map(line => JSON.parse(line));
    assert.equal(deliveries.length,4,'budget checks must fail before starting another process');
    for (const delivery of deliveries) {
      assert.equal(delivery.app,root);
      assert.equal(delivery.model,'external model');
      assert.equal(delivery.provider,undefined);
      assert.deepEqual(JSON.parse(delivery['recipe-task-json']),visible);
      assert.deepEqual(JSON.parse(delivery['skills-json']),[]);
      assert.deepEqual(JSON.parse(delivery['guidance-document-json']),args.guidanceDocument);
    }
  } finally { rmSync(root,{ recursive:true,force:true }); }
});

for (const exitsEarly of [false, true]) test(`standalone adapter deadline stops ${exitsEarly ? 'an orphaned child' : 'an unresponsive agent and its child'}`, {
  skip: exitsEarly && process.platform !== 'linux' ? 'Linux process-group cleanup' : false,
}, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-adapter-deadline-'));
  const entrypoint = join(root, 'agent.mjs');
  const marker = join(root, 'pids.json');
  writeFileSync(entrypoint, `import {spawn} from 'node:child_process';
import {writeFileSync} from 'node:fs';
process.on('SIGTERM',()=>{});
const child=spawn(process.execPath,['-e',"process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"],{stdio:'inherit',detached:${!exitsEarly}});
writeFileSync(${JSON.stringify(marker)},JSON.stringify([process.pid,child.pid]));
${exitsEarly ? 'process.exit(0);' : 'setInterval(()=>{},1000);'}
`);
  const adapter = defineAgentAdapter({ ...AGENT_ADAPTER_REGISTRY.get('deterministic'),
    id: 'external-test', entrypoint, deadlineMs: 2000 });
  const args = { ...parseBenchArguments(['node','bench','--backend','stub','--agent-adapter','deterministic']),
    model: 'external-test', recipeTasks: new Map(), recipeBindings: new Map() };
  let watchdogUsed = false;
  const watchdog = setTimeout(() => {
    watchdogUsed = true;
    if (existsSync(marker)) for (const pid of JSON.parse(readFileSync(marker,'utf8'))) killTree(pid);
  }, 10000);
  try {
    await assert.rejects(runAgent(args, adapter, 'build', 1, root), /agent deadline exceeded/);
    assert.equal(watchdogUsed, false, 'the adapter deadline must stop the complete process tree');
    const pids = JSON.parse(readFileSync(marker,'utf8')) as number[];
    for (const pid of pids) {
      // Linux can retain a killed child as a zombie until its parent is reaped.
      if (process.platform === 'linux' && existsSync(`/proc/${pid}/stat`)
        && /\) Z /.test(readFileSync(`/proc/${pid}/stat`,'utf8'))) continue;
      assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    }
  } finally {
    clearTimeout(watchdog);
    if (existsSync(marker)) for (const pid of JSON.parse(readFileSync(marker,'utf8'))) killTree(pid);
    rmSync(root, { recursive: true, force: true });
  }
});

test('a campaign-bound task supplies the exact recipe when no explicit recipe exists', () => {
  const recipeTask = { schemaVersion: 3,
    recipe: { id: 'ecommerce.sequential-l1' },
    selection: {}, task: {} };
  const recipe = agentRecipeIdentity(null, recipeTask);
  assert.equal(recipe, 'ecommerce.sequential-l1');
  const argv = agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('reference-fixture'),
    { ...request, recipe, recipeTask });
  assert.equal(argv[argv.indexOf('--recipe') + 1], 'ecommerce.sequential-l1');
  assert.throws(() => agentRecipeIdentity('ecommerce.sequential-l2', recipeTask),
    /does not match bound task/);
});

test('completion validation rejects wrong identity and malformed usage', () => {
  const nativeRequest: AgentRequest = { ...request,
    adapterCostLimit: 'native', maxBudgetUsd: 1, pricing };
  const receipt = { schemaVersion: 3, source: 'credential-broker', exact: true, estimatedRequests: 0,
    estimatedByReason: { 'no-usage': 0, 'response-aborted': 0, 'upstream-error': 0 }, model: request.model,
    maxBudgetUsd: 1, costUsd: 0.0018, cliCostUsd: 0.0018, calculatedCostUsd: 0.0018,
    usage: { input: 100, output: 100, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
    pricingRates: { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6,
      cacheRead: 0.3 },
    complete: true, reconciled: true, error: null };
  const valid = { appDir: request.app, mode: 'build', level: 1, ok: true,
    sessionId: 'session-1', costUsd: 0.0018, tokens: 3, outputTokens: 1, turns: 1,
    promptBytes: 20, durationMs: 10, setup: { isolation: { mode: 'test' },
      resources: { buildContainerMemory: {
        currentBytes: 100, peakBytes: 200, limitBytes: 400,
      }, memoryProbeError: null } },
    usage: { input: 1, output: 1, cacheWrite: 1, cacheRead: 0 },
    costReceipts: [{ invocation: 1, receipt }] };
  const normalized = validateAgentResult(valid, nativeRequest);
  assert.equal(normalized.backend, request.backend);
  assert.equal(normalized.model, request.model);
  assert.deepEqual(normalized.transcript, { kind: 'provider-session', id: 'session-1' });
  assert.deepEqual(normalized.costReceipts, valid.costReceipts);
  assert.deepEqual(normalized.setup.resources, valid.setup.resources);
  const resources = { ...valid.setup.resources,
    buildContainerMemory: { ...valid.setup.resources.buildContainerMemory,
      oomEvents: 2, oomKillEvents: 1 },
    buildContainerPids: { current: 7, peak: 512, limit: 512, limitEvents: 129 } };
  assert.deepEqual(validateAgentResult({ ...valid, setup: { resources } }, nativeRequest)
    .setup.resources, resources);
  assert.throws(() => validateAgentResult({ ...valid, setup: { resources: { ...resources,
    buildContainerPids: { ...resources.buildContainerPids, limitEvents: -1 } } } }, nativeRequest),
  /limitEvents/);
  assert.equal(normalized.costComplete, true);
  // Codex reports tokens, not dollar charges; the broker is the cost authority.
  assert.equal(validateAgentResult({ ...valid, costReceipts: [{ invocation: 1,
    receipt: { ...receipt, cliCostUsd: null } }] }, nativeRequest).costComplete, true);
  const routedRequest = { ...nativeRequest, providerRoute: 'openai' };
  const routedReceipt = { ...receipt, costSource: 'provider-reported', provider: 'openrouter',
    providerRoute: 'openai', providerReportedCostUsd: receipt.costUsd,
    upstreamProviders: ['OpenAI'], cliCostUsd: null, calculatedCostUsd: null };
  const routedResult = { ...valid, costReceipts: [{ invocation: 1, receipt: routedReceipt }] };
  assert.equal(validateAgentResult(routedResult, routedRequest).costComplete, true);
  assert.throws(() => validateAgentResult(routedResult, { ...routedRequest, providerRoute: 'other' }),
    /provider route/);
  assert.throws(() => validateAgentResult({ ...routedResult, costReceipts: [{ invocation: 1,
    receipt: { ...routedReceipt, providerReportedCostUsd: 0 } }] }, routedRequest), /cost evidence/);
  const noLedger = validateAgentResult({ ...routedResult, ok: false, costReceipts: [{ invocation: 1,
    receipt: { ...routedReceipt, providerRoute: undefined, providerReportedCostUsd: undefined,
      upstreamProviders: undefined, exact: false, complete: false, reconciled: false,
      error: 'ledger unavailable' } }] }, routedRequest);
  assert.equal(noLedger.costComplete, false);
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
    providerMetadata: { failureCode: 'coding-session-no-output',
      diagnostic: 'coding session failed: permission denied' } });
  assert(failedSession);
  assert.equal(failedSession.kind, 'harness_failure');
  assert.equal(failedSession.reason, 'coding session failed: permission denied');
});

test('malformed and duplicate agent adapters fail at registry construction', () => {
  const source = { schemaVersion: AGENT_ADAPTER_SCHEMA_VERSION, id: 'fake', version: '1.0.0',
    entrypoint: 'fake.mjs', modes: ['build'], deadlineMs: 1000,
    defaultModel: 'fake-model', apiKeyEnvironmentVariable: null,
    credentialEnvironmentVariables: [], credentialFiles: [], outboundDestinations: [],
    requiredExecutables: [],
    credentialStatusCommand: null, usesStackSkills: false, gradesWithFixtureCredentials: false,
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
