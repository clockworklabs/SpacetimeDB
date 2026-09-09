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

function command(argv: string[]) {
  const resolved = resolveControllerCommand(argv);
  assert.ok(resolved);
  return resolved;
}

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

test('controller exposes a small explicit operator command surface', () => {
  assert.equal(resolveControllerCommand([]), null);
  assert.equal(resolveControllerCommand(['--help']), null);
  assert.match(command(['preflight']).args[0] ?? '', /preflight\.js$/);
  const run = command(['run', '--backend', 'postgres', '--levels', '1-2']);
  assert.equal(run.executable, process.execPath);
  assert.match(run.args[0] ?? '', /[\\/]dist[\\/]commands[\\/]/);
  assert.match(run.args[0] ?? '', /bench\.js$/);
  assert.deepEqual(run.args.slice(1), ['--backend', 'postgres', '--levels', '1-2']);
  const recovery = command(['recover', '/private/supervisor.json']);
  assert.match(recovery.args[0] ?? '', /recovery\.js$/);
  assert.deepEqual(recovery.args.slice(1), ['recover', '/private/supervisor.json']);
  const leaseRecovery = command([
    'recover-lease', '/private/backend-lease.json', '--out', '/results/recovered-run']);
  assert.match(leaseRecovery.args[0] ?? '', /recovery\.js$/);
  assert.deepEqual(leaseRecovery.args.slice(1), [
    'recover-lease', '/private/backend-lease.json', '--out', '/results/recovered-run']);
  const campaign = command(['campaign', 'show', '/plans/campaign.json']);
  assert.match(campaign.args[0] ?? '', /campaign-cli\.js$/);
  assert.deepEqual(campaign.args.slice(1), ['show', '/plans/campaign.json']);
  const campaignRun = command(['campaign', 'run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  assert.deepEqual(campaignRun.args.slice(1), ['run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  const dashboard = command(['dashboard', '--port', '7331']);
  assert.match(dashboard.args[0] ?? '', /dashboard[\\/]dashboard-server\.js$/);
  assert.deepEqual(dashboard.args.slice(1), ['--port', '7331']);
  assert.match(command(['qualify-reference']).args[0] ?? '', /reference-live\.js$/);
  assert.match(command(['qualify-null']).args[0] ?? '', /null-control\.js$/);
  assert.match(command(['qualification']).args[0] ?? '', /qualification-cli\.js$/);
  assert.match(command(['pack-budget']).args[0] ?? '', /pack-budget\.js$/);
  assert.match(command(['repair']).args[0] ?? '', /repair-cli\.js$/);
  assert.match(command(['demo']).args[0] ?? '', /demo\.js$/);
  assert.equal(controllerCommandRequiresAgentAuth('demo'), false);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
});

test('controller image starts the compiled entry point', () => {
  const dockerfile = readFileSync(join(STACK_BENCH_ROOT, 'appliance', 'Controller.Dockerfile'),
    'utf8');
  assert.match(dockerfile,
    /ENTRYPOINT \["node", "\/opt\/stack-bench\/dist\/appliance\/controller\.js"\]/);
  assert.doesNotMatch(dockerfile, /ENTRYPOINT .*controller\.ts/);
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
  const modelFree: Array<[string, string[]]> = [
    ['init-deps', []], ['verify-deps', []], ['test', []], ['dashboard', []],
    ['qualify-reference', []], ['qualify-null', []], ['qualification', ['status']],
    ['pack-budget', ['recommend']], ['campaign', ['validate']], ['campaign', ['show']],
    ['campaign', ['trial']], ['campaign', ['status']], ['campaign', ['stop']],
    ['campaign', ['report']], ['campaign', ['reconcile']], ['repair', ['status']],
    ['repair', ['grant']], ['verify-release', []], ['recover', []],
    ['run', ['--grade-from', '/saved/execution', '--out', '/results/regrade']],
    ['run', ['--grade-from', '/saved/execution', '--out', '/results/regrade', '--check', 'saved-check']],
    ['run', ['--grade-from', '/saved/execution', '--grade-level', '2',
      '--out', '/results/regrade', '--check', 'saved-check']],
  ];
  for (const [name, args] of modelFree) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), false,
      `${name} ${args[0] ?? ''}`);
  }
  const paid: Array<[string, string[]]> = [
    ['run', []], ['preflight', []], ['campaign', ['run']], ['campaign', ['resume']], ['campaign', ['extend']],
  ];
  for (const [name, args] of paid) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), true,
      `${name} ${args[0] ?? ''}`);
  }
  assert.throws(() => controllerCommandRequiresAgentAuth('run',
    ['--grade-from', '/saved/execution']), /requires a separate/);
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
    STACK_BENCH_IMAGE: `sha256:${'b'.repeat(64)}`,
    ANTHROPIC_API_KEY: 'ambient-secret',
  });
  assert.equal(command.executable, 'docker');
  assert(command.args.includes(command.containerName));
  assert(command.args.includes(command.ownershipLabel));
  assert.deepEqual(command.args.slice(-6), ['controller', 'campaign', 'resume', 'plan.json', '--out', 'campaigns/demo']);
  assert.equal(command.env.ANTHROPIC_API_KEY, undefined);
  assert.equal(command.env.STACK_BENCH_BUILD_IMAGE, `sha256:${'b'.repeat(64)}`);
});

test('preflight follows the selected adapter and needs no provider credentials for a reference check', () => {
  const scope = ['--backend', 'spacetime,postgres,mongodb', '--levels', '1', '--smoke'];
  assert.equal(controllerCommandRequiresAgentAuth('preflight',
    [...scope, '--agent-adapter', 'reference-fixture']), false);
  assert.equal(controllerCommandRequiresAgentAuth('preflight',
    [...scope, '--agent-adapter=reference-fixture']), false);
  assert.equal(controllerCommandRequiresAgentAuth('preflight', scope), true);
  assert.throws(() => controllerCommandRequiresAgentAuth('preflight',
    [...scope, '--agent-adapter', 'missing-adapter']), /unknown agent adapter/);
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
});
