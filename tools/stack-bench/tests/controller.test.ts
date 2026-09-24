import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import test from 'node:test';
import { parseBenchArguments } from '../commands/bench-arguments.js';

import { controllerChildEnvironment, controllerCommandRequiresAgentAuth,
  controllerRuntimeCommand, controllerRuntimeEnvironment, forwardControllerSignals,
  resolveControllerCommand } from '../appliance/controller.js';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../src/agents/agent-adapters.js';
import { validateCampaignDefinition } from '../src/campaigns/campaign-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('controller forwards repeated stop signals until its child exits', () => {
  const source = new EventEmitter();
  const received: NodeJS.Signals[] = [];
  const stop = forwardControllerSignals({ kill: signal => received.push(signal) }, source);
  source.emit('SIGINT');
  source.emit('SIGINT');
  source.emit('SIGTERM');
  stop();
  source.emit('SIGTERM');
  assert.deepEqual(received, ['SIGINT', 'SIGINT', 'SIGTERM']);
});

test('controller selects exactly one explicit agent credential mode', () => {
  const openai = controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'openai-api-key',
    STACK_BENCH_OPENAI_API_KEY_FILE: '/private/openai', ANTHROPIC_API_KEY: 'ambient',
    CODEX_AUTH_FILE: '/ambient/auth', STACK_BENCH_AGENT_API_KEY: 'ambient' });
  assert.equal(openai.OPENAI_API_KEY_FILE, '/private/openai');
  assert.equal(openai.CODEX_AUTH_FILE, undefined);
  assert.equal(openai.ANTHROPIC_API_KEY, undefined);
  assert.equal(openai.STACK_BENCH_AGENT_API_KEY, undefined);
  const routed = controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'openrouter-api-key',
    STACK_BENCH_OPENROUTER_API_KEY_FILE: '/private/openrouter', OPENAI_API_KEY: 'ambient',
    CODEX_AUTH_FILE: '/ambient/account' });
  assert.equal(routed.OPENROUTER_API_KEY_FILE, '/private/openrouter');
  assert.equal(routed.OPENAI_API_KEY, undefined);
  assert.equal(routed.CODEX_AUTH_FILE, undefined);
  const account = controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'openai-account',
    STACK_BENCH_CODEX_AUTH_FILE: '/private/auth', OPENAI_API_KEY: 'ambient' });
  assert.equal(account.CODEX_AUTH_FILE, '/private/auth');
  assert.equal(account.OPENAI_API_KEY, undefined);
  assert.throws(() => controllerChildEnvironment({}),
    /requires STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE/);
  const subscription = controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'subscription-token',
    STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE: '/private/subscription-token' });
  assert.equal(subscription.CLAUDE_CODE_OAUTH_TOKEN_FILE, '/private/subscription-token');
  assert.throws(() => controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'subscription-token' }),
    /requires STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE/);
  const apiKey = controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'api-key',
    STACK_BENCH_ANTHROPIC_API_KEY_FILE: '/private/key' });
  assert.equal(apiKey.ANTHROPIC_API_KEY_FILE, '/private/key');
  assert.throws(() => controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'api-key' }),
    /requires STACK_BENCH_ANTHROPIC_API_KEY_FILE/);
  assert.throws(() => controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'ambient' }),
    /must be subscription-token or api-key/);
  assert.throws(() => controllerChildEnvironment({ STACK_BENCH_AGENT_AUTH: 'credentials' }),
    /must be subscription-token or api-key/);
});

test('dependency setup does not require or forward agent credentials', () => {
  const env = controllerChildEnvironment({ PATH: '/usr/bin',
    ANTHROPIC_API_KEY: 'ambient-key', CLAUDE_CODE_OAUTH_TOKEN: 'ambient-token' },
  { requireAgentAuth: false });
  assert.equal(env.PATH, '/usr/bin');
  assert.equal(env.ANTHROPIC_API_KEY, undefined);
  assert.equal(env.CLAUDE_CODE_OAUTH_TOKEN, undefined);
});

test('read-only and model-free controller commands do not require agent credentials', () => {
  const scope = ['--backend', 'spacetime,postgres,mongodb', '--levels', '1', '--smoke'];
  const modelFree: Array<[string, string[]]> = [
    ['init-deps', []], ['verify-deps', []], ['test', []], ['dashboard', []], ['demo', []],
    ['qualify-reference', []], ['qualify-null', []], ['qualification', ['status']],
    ['pack-budget', ['recommend']], ['campaign', ['validate']], ['campaign', ['show']],
    ['campaign', ['trial']], ['campaign', ['status']], ['campaign', ['stop']],
    ['campaign', ['report']], ['campaign', ['reconcile']], ['repair', ['status']],
    ['repair', ['grant']], ['verify-release', []], ['recover', []],
    ['run', ['--grade-from', '/saved/execution', '--out', '/results/regrade']],
    ['run', ['--grade-from', '/saved/execution', '--out', '/results/regrade', '--check', 'saved-check']],
    ['run', ['--grade-from', '/saved/execution', '--grade-level', '2',
      '--out', '/results/regrade', '--check', 'saved-check']],
    ['preflight', [...scope, '--agent-adapter', 'reference-fixture']],
    ['preflight', [...scope, '--agent-adapter=reference-fixture']],
  ];
  for (const [name, args] of modelFree) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), false,
      `${name} ${args[0] ?? ''}`);
  }
  const paid: Array<[string, string[]]> = [
    ['run', []], ['preflight', []], ['preflight', scope], ['campaign', ['run']], ['campaign', ['resume']],
    ['campaign', ['extend']],
  ];
  for (const [name, args] of paid) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), true,
      `${name} ${args[0] ?? ''}`);
  }
  assert.throws(() => controllerCommandRequiresAgentAuth('run',
    ['--grade-from', '/saved/execution']), /requires a separate/);
  assert.throws(() => controllerCommandRequiresAgentAuth('preflight',
    [...scope, '--agent-adapter', 'missing-adapter']), /unknown agent adapter/);
  for (const extra of [['--repairs', '1'], ['--model', 'paid-model'],
    ['--agent-adapter', 'claude-code'], ['--campaign-file', '/plans/paid.json'],
    ['--max-budget-usd', '10'], ['--seed-from', '/other/source']]) {
    assert.throws(() => controllerCommandRequiresAgentAuth('run',
      ['--grade-from', '/saved/execution', '--grade-level', '2', '--out', '/results/regrade',
        '--check', 'saved-check', ...extra]), /cannot be combined/);
  }
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'mongodb',
    '--grade-level', '2']), /grade-level/);
  for (const level of ['0', '-1', '1.5', '9007199254740992']) {
    assert.throws(() => controllerCommandRequiresAgentAuth('run',
      ['--grade-from', '/saved/execution', '--out', '/results/regrade', '--grade-level', level,
        '--check', 'saved-check']), /grade-level/);
  }
  for (const duplicate of ['--grade-from=/other/execution', '--grade-level=3']) {
    assert.throws(() => controllerCommandRequiresAgentAuth('run',
      ['--grade-from', '/saved/execution', '--grade-level', '2', '--out', '/results/regrade',
        '--check', 'saved-check', duplicate]), /must be supplied only once/);
  }
  assert.equal(isAbsolute(parseBenchArguments(['node', 'bench', '--grade-from', 'saved',
    '--out', 'regrades/result']).out!), true);
});

test('runtime resolves the configured controller image once and replaces ambient IDs', () => {
  const id = `sha256:${'a'.repeat(64)}`;
  const env = controllerRuntimeEnvironment({ STACK_BENCH_CONTROLLER_IMAGE: 'controller:local',
    STACK_BENCH_CONTROLLER_IMAGE_ID: 'stale' }, reference => {
    assert.equal(reference, 'controller:local');
    return { reference, id };
  });
  assert.equal(env.STACK_BENCH_CONTROLLER_IMAGE_ID, id);
  assert.throws(() => controllerRuntimeEnvironment({}), /is required for runtime work/);
});

test('dashboard runtime launch uses the existing Compose controller with exact ownership', () => {
  const command = controllerRuntimeCommand(['campaign', 'resume', 'plan.json', '--out', 'campaigns/demo'], {
    STACK_BENCH_COMPOSE_FILE: '/opt/stack-bench/appliance/docker-compose.yaml',
    STACK_BENCH_STATE_ROOT: '/var/lib/docker/volumes/stack-bench-state/_data',
    STACK_BENCH_CONTROLLER_IMAGE: `sha256:${'a'.repeat(64)}`,
    STACK_BENCH_BUILD_IMAGE: `sha256:${'b'.repeat(64)}`,
    ANTHROPIC_API_KEY: 'ambient-secret',
  });
  assert.equal(command.executable, 'docker');
  assert(command.args.includes(command.containerName));
  assert(command.args.includes(command.ownershipLabel));
  assert.deepEqual(command.args.slice(-6), ['controller', 'campaign', 'resume', 'plan.json', '--out', 'campaigns/demo']);
  assert.equal(command.env.ANTHROPIC_API_KEY, undefined);
  assert.equal(command.env.STACK_BENCH_BUILD_IMAGE, `sha256:${'b'.repeat(64)}`);
});

test('the delivered model-free plan uses a runtime adapter that survives removal of test fixtures', () => {
  const plan = validateCampaignDefinition(JSON.parse(readFileSync(
    join(STACK_BENCH_ROOT, 'appliance', 'campaign.example.json'), 'utf8')));
  assert(plan.agents.length > 0);
  for (const selection of plan.agents) {
    const adapter = AGENT_ADAPTER_REGISTRY.get(selection.adapter);
    assert.equal(adapter.costLimit, 'non-billable');
    assert.doesNotMatch(adapter.entrypoint, /[/\\]tests[/\\]/);
    assert.match(adapter.entrypoint, /[/\\]src[/\\]references[/\\]reference-agent\.js$/);
    assert.equal(agentAdapterIdentity(adapter).version, selection.adapterVersion);
    assert.match(readFileSync(adapter.entrypoint, 'utf8'), /parseReferenceAgentArgs/);
  }
});

test('named jobs defer credential selection and preserve mixed provider file sources', () => {
  const source = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: '/private/profiles.json',
    ANTHROPIC_API_KEY_FILE: '/private/anthropic', OPENAI_API_KEY_FILE: '/private/openai' };
  assert.deepEqual(controllerChildEnvironment(source), source);
  assert.deepEqual(controllerChildEnvironment(source, { requireAgentAuth: false }), source);
  assert.equal(controllerChildEnvironment({ ...source,
    STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE: '/private/default' }).CLAUDE_CODE_OAUTH_TOKEN_FILE, '/private/default');
  assert.equal(controllerCommandRequiresAgentAuth('job', ['submit']), false);
  assert.equal(controllerCommandRequiresAgentAuth('job', ['work']), true);
  assert.equal(controllerCommandRequiresAgentAuth('job', ['worker']), true);
  assert.match(resolveControllerCommand(['job', 'list'])!.args[0]!, /job-cli.js$/);
  assert.equal(resolveControllerCommand([]), null);
  assert.equal(resolveControllerCommand(['--help']), null);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
});
