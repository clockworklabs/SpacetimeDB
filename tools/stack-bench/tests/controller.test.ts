import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { controllerChildEnvironment, controllerCommandRequiresAgentAuth,
  forwardControllerSignals, resolveControllerCommand } from '../appliance/controller.js';
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
  assert.match(run.args[0] ?? '', /bench\.mjs$/);
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
  assert.match(command(['qualify-reference']).args[0] ?? '', /reference-live\.mjs$/);
  assert.match(command(['qualify-null']).args[0] ?? '', /null-control\.mjs$/);
  assert.match(command(['qualification']).args[0] ?? '', /qualification-cli\.mjs$/);
  assert.match(command(['pack-budget']).args[0] ?? '', /pack-budget\.mjs$/);
  assert.match(command(['repair']).args[0] ?? '', /repair-cli\.js$/);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
});

test('controller image starts the compiled entry point', () => {
  const dockerfile = readFileSync(join(STACK_BENCH_ROOT, 'appliance', 'Controller.Dockerfile'),
    'utf8');
  assert.match(dockerfile,
    /ENTRYPOINT \["node", "\/opt\/stack-bench\/dist\/appliance\/controller\.js"\]/);
  assert.doesNotMatch(dockerfile, /ENTRYPOINT .*controller\.mjs/);
});

test('controller selects exactly one explicit agent credential mode', () => {
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
    ['init-deps', []], ['verify-deps', []], ['test', []],
    ['qualify-reference', []], ['qualify-null', []], ['qualification', ['status']],
    ['pack-budget', ['recommend']], ['campaign', ['validate']], ['campaign', ['show']],
    ['campaign', ['prepare']], ['campaign', ['trial']], ['campaign', ['status']],
    ['campaign', ['report']], ['campaign', ['reconcile']], ['repair', ['status']],
    ['repair', ['grant']], ['verify-release', []], ['recover', []],
  ];
  for (const [name, args] of modelFree) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), false,
      `${name} ${args[0] ?? ''}`);
  }
  const paid: Array<[string, string[]]> = [
    ['run', []], ['preflight', []], ['dashboard', []], ['campaign', ['run']],
  ];
  for (const [name, args] of paid) {
    assert.equal(controllerCommandRequiresAgentAuth(name, args), true,
      `${name} ${args[0] ?? ''}`);
  }
});
