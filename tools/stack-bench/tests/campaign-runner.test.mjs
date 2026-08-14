import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../campaign-compiler.mjs';
import { emptyArtifactIdentities, readArtifact } from '../artifacts.mjs';
import { attemptArgv, campaignExecutionEnvironment, executeCampaign,
  reconcileCampaign, runCampaignAdmission, validateCampaignRun } from '../campaign-runner.mjs';
import { sha256 } from '../provenance.mjs';
import { claimNextAttempt, initializeCampaignDirectory,
  writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');

function frozenRuntime(root) {
  const digests = {
    controller: 'b'.repeat(64),
    'build-sandbox': 'c'.repeat(64),
    postgres: 'd'.repeat(64),
    mongodb: 'e'.repeat(64),
  };
  const images = Object.entries(digests).map(([role, digest]) => ({
    id: `stack-bench-${role}`,
    role,
    reference: `registry.example/stack-bench/${role}@sha256:${digest}`,
    digest,
    platform: 'linux/amd64',
    sbomPath: `sbom/${role}.spdx.json`,
  }));
  const file = (path, role) => ({ path, role, sha256: 'f'.repeat(64), bytes: 1 });
  const manifest = {
    schemaVersion: 2,
    id: 'stack-bench-v1',
    version: '1.0.0',
    state: 'candidate',
    sourceRevision: 'a'.repeat(40),
    sourceSha256: 'a'.repeat(64),
    supportedRunner: { os: 'linux', architecture: 'amd64', stateRoot: '/var/lib/stack-bench',
      networkMode: 'host', dockerSocket: true },
    images,
    files: [file('compose.yaml', 'compose'), file('deps.tar.zst', 'dependency'),
      file('OPERATOR.md', 'operator-guide'), file('secrets.example', 'secrets-template'),
      file('SUPPORT.md', 'support-policy'),
      ...images.map(image => file(image.sbomPath, 'sbom'))],
    outboundDestinations: [],
    secrets: [],
    signing: null,
  };
  const content = `${JSON.stringify(manifest, null, 2)}\n`;
  const path = join(root, 'release.json');
  writeFileSync(path, content);
  return { path, manifest, runtime: {
    releaseManifestSha256: sha256(content),
    controllerImage: images.find(image => image.role === 'controller').reference,
    buildImage: images.find(image => image.role === 'build-sandbox').reference,
    platform: 'linux/amd64',
  } };
}

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

test('campaign validation retains a failed level prefix without accepting partial application results', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts.find(item => item.levels.length > 1) ?? { ...plan.attempts[0], levels: [1, 2] };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter);
  const stack = plan.stacks.find(item => item.id === attempt.stack);
  const run = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      agentAdapter: agent.identity, stackAdapter: stack }) },
  track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  guidance: attempt.guidance, selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 }, levels: [{ level: 1 }],
  outcome: { kind: 'harness_failure', reason: 'provider-session-error' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, outcome: { kind: 'app_failure' } }, { buildImage: 'test-build-image' }),
  /does not match.*levels/);
});

test('campaign validation accepts a zero-level interrupted run without invented cost totals', () => {
  const compiled = compileCampaignFile(example);
  const plan = { ...compiled, definition: { ...compiled.definition,
    budgets: { ...compiled.definition.budgets, maxCostUsdPerAttempt: 100 } } };
  const attempt = { ...plan.attempts[0], levels: [1, 2] };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter);
  const stack = plan.stacks.find(item => item.id === attempt.stack);
  const run = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      agentAdapter: agent.identity, stackAdapter: stack }) },
  track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  guidance: attempt.guidance, selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, levels: [],
  outcome: { kind: 'ungraded', reason: 'coding session did not run' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, outcome: { kind: 'app_failure' } }, { buildImage: 'test-build-image' }),
  /does not match.*levels.*totals\.costUsd/);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, backend: 'wrong' }, { buildImage: 'test-build-image' }),
  /does not match.*backend/);
});

test('draft campaigns cannot start unless an explicit test-only caller permits them', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-draft-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { execute: async () => {
      throw new Error('must not launch');
    } }), /requires a frozen plan/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('failed campaign admission leaves every attempt pending and unclaimed', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-admission-fail-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { allowDraft: true,
      admit: () => ({ id: 'failed-admission', payload: { ok: false } }),
      execute: async () => { throw new Error('must not launch'); },
    }), /admission failed/);
    const { readCampaignState } = await import('../campaign-scheduler.mjs');
    const state = readCampaignState(root).state;
    assert.equal(state.status, 'prepared');
    assert.equal(state.summary.executions, 0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a frozen campaign proves its release and both runtime images before admission', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runtime-'));
  try {
    const { path, manifest, runtime } = frozenRuntime(root);
    const plan = { state: 'frozen', definition: { runtime } };
    const env = { STACK_BENCH_CONTROLLER_IMAGE: runtime.controllerImage,
      STACK_BENCH_RELEASE_MANIFEST: path };
    assert.equal(campaignExecutionEnvironment(plan, env).STACK_BENCH_IMAGE, runtime.buildImage);
    const internalPlan = { state: 'frozen', definition: { runtime: {
      ...runtime, releaseManifestSha256: null,
    } } };
    assert.equal(campaignExecutionEnvironment(internalPlan, {
      STACK_BENCH_CONTROLLER_IMAGE: runtime.controllerImage,
    }).STACK_BENCH_IMAGE, runtime.buildImage);
    assert.throws(() => campaignExecutionEnvironment(plan, { ...env,
      STACK_BENCH_CONTROLLER_IMAGE: `registry.example/controller@sha256:${'1'.repeat(64)}` }),
    /controller image does not match/);
    assert.throws(() => campaignExecutionEnvironment(plan, { ...env,
      STACK_BENCH_IMAGE: `registry.example/build@sha256:${'2'.repeat(64)}` }), /conflicts/);
    const wrongImages = structuredClone(manifest);
    const wrongController = wrongImages.images.find(image => image.role === 'controller');
    wrongController.digest = '1'.repeat(64);
    wrongController.reference = `registry.example/stack-bench/controller@sha256:${wrongController.digest}`;
    const wrongContent = `${JSON.stringify(wrongImages, null, 2)}\n`;
    writeFileSync(path, wrongContent);
    assert.throws(() => campaignExecutionEnvironment({ state: 'frozen', definition: {
      runtime: { ...runtime, releaseManifestSha256: sha256(wrongContent) },
    } }, env), /release manifest images do not match/);
    writeFileSync(path, '{}\n');
    assert.throws(() => campaignExecutionEnvironment(plan, env), /release manifest does not match/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('model-free campaign execution checkpoints a retry and every completed attempt', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-'));
  const calls = [];
  const planned = compileCampaignFile(example);
  try {
    const state = await executeCampaign(example, root, { allowDraft: true,
      admit: () => ({ id: 'admission-1', payload: { ok: true } }),
      execute: async (command, argv, options) => {
        calls.push({ command, argv, options });
        const output = argv[argv.indexOf('--out') + 1];
        assert.equal(existsSync(output), true,
          'every execution, including a retry, must have a preflight-mountable output directory');
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
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:30.000Z', uuid: () => 'reconcile',
      preflight: request => ({ schemaVersion: 1, generatedAt: '2026-08-12T00:00:30.000Z',
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    const running = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: admission.id }).state;
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

test('campaign admission covers every stack once per distinct agent adapter and writes typed evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-admission-'));
  try {
    const plan = compileCampaignFile(example);
    const calls = [];
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:00.000Z', uuid: () => 'test',
      preflight: request => {
        calls.push(request);
        return { schemaVersion: 1, generatedAt: '2026-08-12T00:00:00.000Z',
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
      },
    });
    assert.equal(admission.payload.ok, true);
    assert.deepEqual(admission.payload.runtime, plan.definition.runtime);
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0].backends, ['mongodb', 'postgres', 'spacetime']);
    assert.equal(calls[0].smoke, true);
    assert.equal(readArtifact(admission.path,
      { expectedKind: 'campaign_admission' }).payload.campaignSha256, plan.contentSha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
