import assert from 'node:assert/strict';
import test from 'node:test';

import { controllerChildEnvironment, resolveControllerCommand } from '../appliance/controller.mjs';

test('controller exposes a small explicit operator command surface', () => {
  assert.equal(resolveControllerCommand([]), null);
  assert.equal(resolveControllerCommand(['--help']), null);
  const run = resolveControllerCommand(['run', '--backend', 'postgres', '--levels', '1-2']);
  assert.equal(run.executable, process.execPath);
  assert.match(run.args[0], /bench\.mjs$/);
  assert.deepEqual(run.args.slice(1), ['--backend', 'postgres', '--levels', '1-2']);
  const recovery = resolveControllerCommand(['recover', '/private/supervisor.json']);
  assert.match(recovery.args[0], /recovery\.mjs$/);
  assert.deepEqual(recovery.args.slice(1), ['recover', '/private/supervisor.json']);
  const campaign = resolveControllerCommand(['campaign', 'show', '/plans/campaign.json']);
  assert.match(campaign.args[0], /campaign-cli\.mjs$/);
  assert.deepEqual(campaign.args.slice(1), ['show', '/plans/campaign.json']);
  const campaignRun = resolveControllerCommand(['campaign', 'run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  assert.deepEqual(campaignRun.args.slice(1), ['run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  const dashboard = resolveControllerCommand(['dashboard', '--port', '7331']);
  assert.match(dashboard.args[0], /dashboard[\\/]dashboard-server\.mjs$/);
  assert.deepEqual(dashboard.args.slice(1), ['--port', '7331']);
  const reference = resolveControllerCommand(['qualify-reference', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1']);
  assert.match(reference.args[0], /reference-live\.mjs$/);
  const nullControl = resolveControllerCommand(['qualify-null', '--track', 'ecommerce', '--level', '1']);
  assert.match(nullControl.args[0], /null-control\.mjs$/);
  const qualification = resolveControllerCommand(['qualification', 'status',
    '--track', 'ecommerce', '--level', '1']);
  assert.match(qualification.args[0], /qualification-cli\.mjs$/);
  const budget = resolveControllerCommand(['pack-budget', 'recommend', '--track', 'ecommerce',
    '--level', '1', '--evidence', '/results/mongodb.json', '--out', '/results/budgets.json']);
  assert.match(budget.args[0], /pack-budget\.mjs$/);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
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
