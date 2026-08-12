import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../campaign-compiler.mjs';
import { attemptArgv, campaignExecutionEnvironment, executeCampaign,
  reconcileCampaign } from '../campaign-runner.mjs';
import { claimNextAttempt, initializeCampaignDirectory,
  writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');

test('attempt argv is derived completely from the compiled campaign plan', () => {
  const plan = compileCampaignFile(example);
  const argv = attemptArgv(plan, plan.attempts[0], '/campaign/attempt');
  assert.deepEqual(argv.slice(1), [
    '--backend', plan.attempts[0].stack,
    '--track', 'ecommerce', '--levels', '1-1', '--run-index', '0',
    '--out', '/campaign/attempt', '--agent-adapter', 'deterministic',
    '--model', 'deterministic', '--guidance', 'prescribed', '--fix-rounds', '3',
    '--parent-attempt-id', plan.attempts[0].id, '--no-media',
  ]);
});

test('draft campaigns cannot start unless an explicit test-only caller permits them', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-draft-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { execute: async () => {
      throw new Error('must not launch');
    } }), /requires a frozen plan/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a frozen runtime image cannot be replaced by ambient controller state', () => {
  const buildImage = `registry.example/build@sha256:${'c'.repeat(64)}`;
  const plan = { definition: { runtime: { buildImage } } };
  assert.equal(campaignExecutionEnvironment(plan, {}).STACK_BENCH_IMAGE, buildImage);
  assert.throws(() => campaignExecutionEnvironment(plan,
    { STACK_BENCH_IMAGE: `registry.example/build@sha256:${'d'.repeat(64)}` }), /conflicts/);
});

test('model-free campaign execution checkpoints a retry and every completed attempt', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-'));
  const calls = [];
  const planned = compileCampaignFile(example);
  try {
    const state = await executeCampaign(example, root, { allowDraft: true,
      execute: async (command, argv, options) => {
        calls.push({ command, argv, options });
        const output = argv[argv.indexOf('--out') + 1];
        const parent = argv[argv.indexOf('--parent-attempt-id') + 1];
        const { emptyArtifactIdentities, writeRunJson } = await import('../artifacts.mjs');
        if (calls.length === 1) return { code: 1, timedOut: false };
        const completedAt = new Date().toISOString();
        const attempt = planned.attempts.find(item => item.id === parent);
        const agent = planned.agents.find(item => item.adapter === attempt.agentAdapter);
        const stack = planned.stacks.find(item => item.id === attempt.stack);
        writeRunJson(join(output, 'run.json'), { id: `fake-${calls.length}`,
          parentAttemptId: parent, startedAt: completedAt, completedAt,
          identities: emptyArtifactIdentities({ agentAdapter: agent.identity,
            stackAdapter: stack }),
          track: planned.definition.track, backend: attempt.stack, model: attempt.model,
          guidance: attempt.guidance, selectionRequest: planned.definition.selection,
          skills: attempt.skills, runtime: { buildImage: options.env.STACK_BENCH_IMAGE },
          totals: { costUsd: 0 },
          levels: attempt.levels.map(level => ({ level })),
          outcome: { kind: 'passed' } });
        return { code: 0, timedOut: false };
      } });
    assert.equal(state.status, 'completed');
    assert.equal(state.summary.completed, 9);
    assert.equal(state.summary.executions, 10);
    assert.deepEqual(state.attempts[0].executions.map(item => item.status), ['invalid', 'completed']);
    assert.equal(calls.every(call => call.options.timeoutMs === 240 * 60_000), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('interrupted work advances only after exact supervisor cleanup is proven', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-reconcile-'));
  try {
    const plan = compileCampaignFile(example);
    const initialized = initializeCampaignDirectory(plan, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const running = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z' }).state;
    writeCampaignState(initialized.paths.state, plan, running);
    assert.throws(() => reconcileCampaign(example, root, { rescue: () => {} }), /no private supervisor/);
    const execution = running.attempts[0].executions[0];
    const privateDir = join(root, '.private');
    mkdirSync(privateDir);
    writeFileSync(join(privateDir, `${execution.id}.supervisor.json`), '{}');
    let rescued = false;
    const state = reconcileCampaign(example, root, { rescue: () => { rescued = true; } });
    assert.equal(rescued, true);
    assert.equal(state.attempts[0].status, 'invalid');
    assert.equal(state.attempts[0].executions[0].outcome, 'scheduler_interrupted');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
