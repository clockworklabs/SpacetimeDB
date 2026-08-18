import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../campaign-compiler.mjs';
import { emptyArtifactIdentities, readArtifact, writeArtifact } from '../artifacts.mjs';
import { attemptArgv, campaignExecutionEnvironment, campaignSlotEnvironment, executeCampaign,
  reconcileCampaign, runCampaignAdmission, validateCampaignRun } from '../campaign-runner.mjs';
import { sha256 } from '../provenance.mjs';
import { claimNextAttempt, initializeCampaignDirectory,
  writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');
const productBrief = join(import.meta.dirname, '..', 'appliance',
  'campaign.product-brief-reference.json');

test('parallel SpacetimeDB slots receive distinct dedicated host ports', () => {
  assert.equal(campaignSlotEnvironment({}, 'spacetime', 0).STACK_BENCH_STDB_URI,
    'http://127.0.0.1:3210');
  assert.equal(campaignSlotEnvironment({}, 'spacetime', 7).STACK_BENCH_STDB_URI,
    'http://127.0.0.1:3217');
  assert.equal(campaignSlotEnvironment({ STACK_BENCH_STDB_URI: 'http://localhost:4100' },
    'spacetime', 2).STACK_BENCH_STDB_URI, 'http://localhost:4102');
  assert.equal(campaignSlotEnvironment({ KEEP: 'yes' }, 'postgres', 2).KEEP, 'yes');
  assert.throws(() => campaignSlotEnvironment({ STACK_BENCH_STDB_URI: 'https://example.com' },
    'spacetime', 1), /explicit loopback port/);
});

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
  const argv = attemptArgv(plan, plan.attempts[0], '/campaign/attempt', 0);
  assert.deepEqual(argv.slice(1), [
    '--backend', plan.attempts[0].stack,
    '--track', 'ecommerce', '--levels', '1-1', '--run-index', '0',
    '--out', '/campaign/attempt', '--agent-adapter', 'deterministic',
    '--model', 'deterministic', '--guidance', 'prescribed',
    '--guidance-document-json', JSON.stringify(plan.attempts[0].condition.guidance.documents[
      plan.attempts[0].stack]),
    '--condition-json', JSON.stringify(plan.attempts[0].condition),
    '--selection-json', JSON.stringify(plan.definition.selection), '--fix-rounds', '3',
    '--parent-attempt-id', plan.attempts[0].id, '--no-media',
    '--skills-json', JSON.stringify(plan.attempts[0].skills),
  ]);
  assert.throws(() => attemptArgv(plan, { ...plan.attempts[0], condition: {
    ...plan.attempts[0].condition, guidance: { ...plan.attempts[0].condition.guidance,
      documents: {} },
  } }, '/campaign/attempt', 0), /has no guidance document/);
  assert.throws(() => attemptArgv(plan, plan.attempts[0], '/campaign/attempt'), /requires a run slot/);
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
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
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
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
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

test('campaign validation rejects an application result that stopped before its correction budget', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts[0];
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter);
  const stack = plan.stacks.find(item => item.id === attempt.stack);
  const run = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      agentAdapter: agent.identity, stackAdapter: stack }) },
  track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, score: 0, max: 58, fixRounds: 1,
    firstBuild: { score: 0, max: 58, outcome: { kind: 'app_failure' } },
    repair: { status: 'incomplete', budgetRounds: 3, roundsUsed: 1, stopReason: null },
    outcome: { kind: 'app_failure' } }], outcome: { kind: 'app_failure' } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.repair/);
  run.levels[0] = { ...run.levels[0], fixRounds: 3,
    repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3, stopReason: null } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  run.levels[0] = { ...run.levels[0], score: 58 };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.score/);
  run.levels[0] = { ...run.levels[0], regression: { score: 0, max: 1 } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run,
    'a perfect requested score may still lose an inherited guarantee');
  run.levels[0] = { ...run.levels[0], regression: null,
    outcome: { kind: 'app_failure', appFailures: ['systems/diagnostic'] } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.score/,
    'test-development evidence cannot turn a perfect scored result into an application failure');
  run.levels[0] = { ...run.levels[0], score: 0, outcome: { kind: 'passed' },
    repair: { status: 'corrected', budgetRounds: 3, roundsUsed: 3, stopReason: null } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /outcome\.kind/);
});

test('campaign validation requires complete first-build and final measurement coverage', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts[0];
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter);
  const stack = plan.stacks.find(item => item.id === attempt.stack);
  const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
  const run = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      agentAdapter: agent.identity, stackAdapter: stack }) },
  track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, score: 58, max: 58, fixRounds: 0,
    firstBuild: { score: 58, max: 58, outcome },
    repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
    outcome }], outcome: { kind: 'passed' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);

  const missingFirstBuildPoint = structuredClone(run);
  missingFirstBuildPoint.levels[0].firstBuild.score = 57;
  missingFirstBuildPoint.levels[0].firstBuild.max = 57;
  assert.throws(() => validateCampaignRun(plan, attempt, missingFirstBuildPoint,
    { buildImage: 'test-build-image' }), /firstBuild\.max/);

  const missingFinalPoint = structuredClone(run);
  missingFinalPoint.levels[0].max = 57;
  missingFinalPoint.levels[0].score = 57;
  assert.throws(() => validateCampaignRun(plan, attempt, missingFinalPoint,
    { buildImage: 'test-build-image' }), /levels\.L1\.max/);

  const inconclusiveFirstBuild = structuredClone(run);
  inconclusiveFirstBuild.levels[0].firstBuild.outcome = { kind: 'app_failure',
    appFailures: ['feature/accounts'],
    inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] };
  assert.throws(() => validateCampaignRun(plan, attempt, inconclusiveFirstBuild,
    { buildImage: 'test-build-image' }), /firstBuild\.outcome\.inconclusive/);

  const inconclusiveFinal = structuredClone(run);
  inconclusiveFinal.levels[0].outcome = { kind: 'app_failure',
    appFailures: ['feature/accounts'],
    inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] };
  inconclusiveFinal.outcome = { kind: 'app_failure' };
  inconclusiveFinal.levels[0].score = 57;
  inconclusiveFinal.levels[0].fixRounds = 3;
  inconclusiveFinal.levels[0].repair = { status: 'budget-exhausted', budgetRounds: 3,
    roundsUsed: 3, stopReason: null };
  assert.throws(() => validateCampaignRun(plan, attempt, inconclusiveFinal,
    { buildImage: 'test-build-image' }), /final\.outcome\.inconclusive/);
});

test('campaign validation binds observed-only evidence to its exact first-build selection', () => {
  const compiled = compileCampaignFile(example);
  const sourceSha256 = 'a'.repeat(64);
  const selectionSha256 = 'b'.repeat(64);
  const scoredChecks = [
    { stableKey: 'feature/accounts', points: 1, treatment: 'requested' },
    { stableKey: 'authorization/session', points: 1, treatment: 'expected' },
  ];
  const observedChecks = [
    { stableKey: 'durability/session', points: 1, treatment: 'observed' },
  ];
  const selectedObservedKeys = observedChecks.map(check => check.stableKey);
  const specifications = { requested: [], expected: ['authorization@1.0.0'],
    observed: ['durability@1.0.0'] };
  const baseAttempt = compiled.attempts[0];
  const condition = { ...baseAttempt.condition, requested: { levels: [{ level: 1,
    selection: { schemaVersion: 3, sha256: selectionSha256, scoredPoints: 2,
      specifications, scoredChecks, observedChecks } }] } };
  const plan = { ...compiled, conditions: compiled.conditions.map(item =>
    item.sha256 === condition.sha256 ? condition : item) };
  const attempt = { ...baseAttempt, condition };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter);
  const stack = plan.stacks.find(item => item.id === attempt.stack);
  const run = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      agentAdapter: agent.identity, stackAdapter: stack }) },
  track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, score: 0, max: 2, fixRounds: 3,
    selection: { schemaVersion: 3, sha256: selectionSha256, scoredPoints: 2,
      specifications,
      scoredChecks, observedChecks },
    firstBuild: { score: 0, max: 2, outcome: { kind: 'app_failure' },
      source: { sha256: sourceSha256, files: 1 }, observations: {
      sourceSha256, selectionSha256, selectedChecks: selectedObservedKeys,
      reportedChecks: selectedObservedKeys, passedPoints: 1, observedPoints: 1,
      scoreContribution: false, repairVisible: false,
      artifact: 'first-build-l1-observed/bundle.json', outcome: { kind: 'passed' },
    } },
    repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3,
      stopReason: null }, outcome: { kind: 'app_failure' } }],
  outcome: { kind: 'app_failure' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  const wrongExpected = structuredClone(run);
  wrongExpected.levels[0].selection.specifications.expected = [];
  assert.throws(() => validateCampaignRun(plan, attempt, wrongExpected,
    { buildImage: 'test-build-image' }), /selection\.specifications/);
  const visibleToRepair = structuredClone(run);
  visibleToRepair.levels[0].firstBuild.observations.repairVisible = true;
  assert.throws(() => validateCampaignRun(plan, attempt, visibleToRepair,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.repairVisible/);
  const wrongSource = structuredClone(run);
  wrongSource.levels[0].firstBuild.observations.sourceSha256 = 'c'.repeat(64);
  assert.throws(() => validateCampaignRun(plan, attempt, wrongSource,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.sourceSha256/);
  const wrongDenominator = structuredClone(run);
  wrongDenominator.levels[0].firstBuild.observations.observedPoints = 2;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongDenominator,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.observedPoints/);
  const wrongWeight = structuredClone(run);
  wrongWeight.levels[0].selection.observedChecks[0].points = 2;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongWeight,
    { buildImage: 'test-build-image' }), /selection\.observedChecks/);
  const wrongScoreDenominator = structuredClone(run);
  wrongScoreDenominator.levels[0].max = 1;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongScoreDenominator,
    { buildImage: 'test-build-image' }), /levels\.L1\.max/);
  const wrongPlannedSelection = structuredClone(run);
  wrongPlannedSelection.levels[0].selection.observedChecks[0].stableKey = 'durability/cart';
  wrongPlannedSelection.levels[0].firstBuild.observations.selectedChecks = ['durability/cart'];
  wrongPlannedSelection.levels[0].firstBuild.observations.reportedChecks = ['durability/cart'];
  assert.throws(() => validateCampaignRun(plan, attempt, wrongPlannedSelection,
    { buildImage: 'test-build-image' }), /selection\.observedChecks/);
});

test('ordinary campaign execution refuses draft plans', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-draft-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { execute: async () => {
      throw new Error('must not launch');
    } }), /requires a frozen plan/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign trials accept only non-billable draft plans with zero pricing', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-trial-policy-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, {
      mode: 'model-free-trial',
      admit: () => ({ id: 'failed-admission', payload: { ok: false } }),
      execute: async () => { throw new Error('must not launch'); },
    }), /admission failed/);

    const paid = JSON.parse(readFileSync(example, 'utf8'));
    paid.agents = [{ adapter: 'claude-code', adapterVersion: '1.9.0', model: 'claude-sonnet-5' }];
    paid.pricing.models = { 'claude-sonnet-5': {
      inputPerMillion: 0, outputPerMillion: 0,
      cacheWritePerMillion: 0, cacheReadPerMillion: 0,
    } };
    const paidPath = join(root, 'paid.json');
    writeFileSync(paidPath, `${JSON.stringify(paid, null, 2)}\n`);
    await assert.rejects(() => executeCampaign(paidPath, join(root, 'paid'), {
      mode: 'model-free-trial',
      execute: async () => { throw new Error('must not launch'); },
    }), /requires non-billable agent adapters/);

    const priced = JSON.parse(readFileSync(example, 'utf8'));
    priced.pricing.models.deterministic.inputPerMillion = 1;
    const pricedPath = join(root, 'priced.json');
    writeFileSync(pricedPath, `${JSON.stringify(priced, null, 2)}\n`);
    await assert.rejects(() => executeCampaign(pricedPath, join(root, 'priced'), {
      mode: 'model-free-trial',
      execute: async () => { throw new Error('must not launch'); },
    }), /requires zero pricing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('failed campaign admission leaves every attempt pending and unclaimed', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-admission-fail-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { mode: 'model-free-trial',
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
    const state = await executeCampaign(example, root, { mode: 'model-free-trial',
      admit: () => ({ id: 'admission-1', payload: { ok: true } }),
      execute: async (command, argv, options) => {
        calls.push({ command, argv, options });
        const output = argv[argv.indexOf('--out') + 1];
        assert.equal(existsSync(output), true,
          'every execution, including a retry, must have a preflight-mountable output directory');
        assert.deepEqual(options.logs, {
          stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log'),
        });
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
          guidance: attempt.guidance, condition: attempt.condition,
          selectionRequest: planned.definition.selection,
          skills: attempt.skills, runtime: { buildImage: options.env.STACK_BENCH_IMAGE },
          totals: { costUsd: 0 },
          levels: attempt.levels.map(level => {
            const max = attempt.condition.requested.levels
              .find(item => item.level === level).selection.scoredPoints;
            const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
            return { level, score: max, max, fixRounds: 0,
              firstBuild: { score: max, max, outcome },
            repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
              outcome };
          }),
          outcome: { kind: 'passed' } });
        return { code: 0, timedOut: false };
      } });
    assert.equal(state.status, 'completed');
    assert.equal(state.summary.completed, 9);
    assert.equal(state.summary.executions, 10);
    assert.deepEqual(state.attempts[0].executions.map(item => item.status), ['invalid', 'completed']);
    assert.equal(calls.every(call => call.options.timeoutMs === 240 * 60_000), true);
    const processArtifact = readArtifact(join(root, state.attempts[0].executions[0].output, 'process.json'),
      { expectedKind: 'campaign_process' });
    assert.equal(processArtifact.payload.executionId, state.attempts[0].executions[0].id);
    assert.equal(processArtifact.payload.exitCode, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('one campaign runs multiple attempts of the same stack concurrently in isolated slots', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-parallel-'));
  try {
    const definition = JSON.parse(readFileSync(example, 'utf8'));
    definition.id = 'postgres-parallel-proof';
    definition.repetitions = 1;
    definition.parallelism = 3;
    definition.stacks = [definition.stacks.find(stack => stack.id === 'postgres')];
    definition.stacks[0].repetitions = 3;
    const campaignPath = join(root, 'campaign.json');
    const results = join(root, 'results');
    writeFileSync(campaignPath, `${JSON.stringify(definition, null, 2)}\n`);
    const planned = compileCampaignFile(campaignPath);
    let active = 0;
    let maxActive = 0;
    const started = [];
    let releaseFirstWave;
    const firstWave = new Promise(resolve => { releaseFirstWave = resolve; });
    const state = await executeCampaign(campaignPath, results, { mode: 'model-free-trial',
      admit: () => ({ id: 'parallel-admission', payload: { ok: true } }),
      execute: async (command, argv, options) => {
        const runIndex = Number(argv[argv.indexOf('--run-index') + 1]);
        const parent = argv[argv.indexOf('--parent-attempt-id') + 1];
        const output = argv[argv.indexOf('--out') + 1];
        active += 1;
        maxActive = Math.max(maxActive, active);
        started.push({ parent, runIndex });
        if (started.length === 3) releaseFirstWave();
        await firstWave;
        const attempt = planned.attempts.find(item => item.id === parent);
        const agent = planned.agents.find(item => item.adapter === attempt.agentAdapter);
        const stack = planned.stacks.find(item => item.id === attempt.stack);
        const completedAt = new Date().toISOString();
        const { writeRunJson } = await import('../artifacts.mjs');
        writeRunJson(join(output, 'run.json'), { id: `parallel-${runIndex}`,
          parentAttemptId: parent, startedAt: completedAt, completedAt,
          identities: emptyArtifactIdentities({ agentAdapter: agent.identity,
            stackAdapter: stack }),
          track: planned.definition.track, backend: attempt.stack, model: attempt.model,
          guidance: attempt.guidance, condition: attempt.condition,
          selectionRequest: planned.definition.selection, skills: attempt.skills,
          runtime: { buildImage: options.env.STACK_BENCH_IMAGE }, totals: { costUsd: 0 },
          levels: attempt.levels.map(level => {
            const max = attempt.condition.requested.levels
              .find(item => item.level === level).selection.scoredPoints;
            const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
            return { level, score: max, max, fixRounds: 0,
              firstBuild: { score: max, max, outcome },
              repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
              outcome };
          }), outcome: { kind: 'passed' } });
        active -= 1;
        return { code: 0, timedOut: false };
      } });
    assert.equal(maxActive, 3);
    assert.deepEqual(started.map(item => item.runIndex).sort((a, b) => a - b), [0, 1, 2]);
    assert.equal(new Set(started.map(item => item.parent)).size, 3);
    assert(state.attempts.every(attempt => attempt.status === 'completed'));
    assert(state.attempts.every(attempt => attempt.executions[0].runIndex >= 0
      && attempt.executions[0].runIndex < 3));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('interrupted parallel work advances only after every exact cleanup is proven', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-reconcile-'));
  try {
    const definition = JSON.parse(readFileSync(example, 'utf8'));
    definition.parallelism = 2;
    definition.repetitions = 1;
    const campaignPath = join(root, 'parallel.json');
    writeFileSync(campaignPath, `${JSON.stringify(definition, null, 2)}\n`);
    const plan = compileCampaignFile(campaignPath);
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
    let running = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: admission.id }).state;
    running = claimNextAttempt(running, { now: '2026-08-12T00:01:01.000Z',
      admissionId: admission.id }).state;
    writeCampaignState(initialized.paths.state, plan, running);
    assert.throws(() => reconcileCampaign(campaignPath, root, { rescue: () => {} }),
      /neither private supervisor authority nor public clean recovery proof/);
    const privateDir = join(root, '.private');
    mkdirSync(privateDir);
    const executions = running.attempts.filter(attempt => attempt.status === 'running')
      .map(attempt => attempt.executions.at(-1));
    for (const execution of executions) {
      writeFileSync(join(privateDir, `${execution.id}.supervisor.json`), '{}');
    }
    const rescued = [];
    const state = reconcileCampaign(campaignPath, root,
      { rescue: supervisor => { rescued.push(supervisor); } });
    assert.equal(rescued.length, 2);
    assert.equal(state.summary.invalid, 2);
    assert(state.attempts.filter(attempt => attempt.status === 'invalid')
      .every(attempt => attempt.executions[0].outcome === 'scheduler_interrupted'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reconciliation accepts the clean public proof left by authenticated recovery', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-recovered-'));
  try {
    const plan = compileCampaignFile(example);
    const initialized = initializeCampaignDirectory(plan, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:30.000Z', uuid: () => 'recovered',
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
    const execution = running.attempts[0].executions[0];
    const output = join(root, execution.output);
    mkdirSync(output, { recursive: true });
    writeArtifact(join(output, 'recovery.json'), { kind: 'recovery', id: 'recovered-run-recovery',
      attempt: { id: 'recovered-run-recovery', parentId: 'recovered-run' },
      identities: emptyArtifactIdentities({ stackAdapter: { id: running.attempts[0].plan.stack } }),
      payload: { schemaVersion: 1, status: 'clean', runId: 'recovered-run',
        backend: running.attempts[0].plan.stack, reason: null,
        cleanup: { succeeded: true, retained: false },
        resources: { backendState: 'released', buildContainer: { id: 'container-id',
          name: 'container-name', running: false }, listenerPids: [],
        locks: [{ key: 'slot:test', released: true }] },
        instructions: ['No recovery action is required.'] } });
    const state = reconcileCampaign(example, root, {
      rescue: () => { throw new Error('private recovery must not run without private authority'); },
    });
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
    assert.deepEqual([...calls[0].backends].sort(), ['mongodb', 'postgres', 'spacetime']);
    assert.equal(new Set(calls[0].backends).size, 3);
    assert.equal(calls[0].smoke, true);
    assert.equal(readArtifact(admission.path,
      { expectedKind: 'campaign_admission' }).payload.campaignSha256, plan.contentSha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign admission accepts a modular level selection without legacy pack filters', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-modular-campaign-admission-'));
  try {
    const plan = compileCampaignFile(productBrief);
    const calls = [];
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:00.000Z', uuid: () => 'modular',
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
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0].packIds, []);
    assert.deepEqual(calls[0].checkKeys, []);
    assert.equal(calls[0].requestedScopes[0].levels[0].selection.schemaVersion, 3);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
