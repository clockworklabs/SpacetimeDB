import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  compileCampaignFile,
  campaignProgressionOwner,
} from '../src/campaigns/campaign-compiler.js';
import {
  claimNextAttempt,
  finishCampaignExecution,
  initializeCampaignDirectory,
  writeCampaignState,
} from '../src/campaigns/campaign-scheduler.js';
import { generateCampaignReport } from '../src/campaigns/campaign-report.js';
import { createCampaignUpload, publishCampaign } from '../src/campaigns/campaign-publish.js';
import {
  artifactPayload,
  emptyArtifactIdentities,
  writeArtifact,
} from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { classifyBundle } from '../src/evidence/outcomes.js';
import { hashDirectory } from '../src/evidence/provenance.js';
import {
  compileProgressionInput,
  dependencyRuntimeDefinition,
} from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { dependencyLevelRepairRecords } from '../src/progression/dependency-mode.js';
import { liveProgressionStatus } from '../src/progression/live-progression.js';
import { writeProgressionState } from '../src/progression/progression-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('retained dependency evidence exports through the CLI and sends only validated private uploads', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stackbench-publish-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const definition = JSON.parse(
    readFileSync(
      join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'),
      'utf8'
    )
  );
  definition.state = 'frozen';
  definition.budgets.maxCostUsdPerAttempt = 10;
  const buildImage = `sha256:${'b'.repeat(64)}`;
  definition.runtime.controllerImage = `sha256:${'c'.repeat(64)}`;
  definition.runtime.buildImage = buildImage;
  definition.repair.budget = { total: 0, perFeature: 0 };
  definition.featureCatalog.nodes[1].dependencies = [
    { id: 'accounts', reason: 'Catalog needs an account' },
  ];
  definition.selection.levels.push({ level: 2, recipe: 'ecommerce.sequential-l2' });
  definition.repetitions = 2;
  const manifest = join(root, 'fixture.json');
  writeFileSync(manifest, JSON.stringify(definition));
  const plan = compileCampaignFile(manifest);
  assert.ok(plan.featureCatalog && plan.dependencyPolicy);
  const campaign = join(root, 'campaign');
  const now = '2026-10-14T12:00:00.000Z';
  let { state } = initializeCampaignDirectory(plan, campaign, { now });
  assert.throws(() => createCampaignUpload(campaign, 'fixture-suite'), /finished/);
  writeArtifact(join(campaign, 'admissions/fixture.json'), {
    kind: 'campaign_admission',
    id: 'fixture',
    identities: emptyArtifactIdentities({
      experiment: {
        id: plan.id,
        version: plan.version,
        sha256: plan.contentSha256,
        state: plan.state,
      },
    }),
    payload: {
      schemaVersion: 1,
      campaignId: plan.id,
      campaignSha256: plan.contentSha256,
      createdAt: now,
      ok: true,
      runtime: plan.definition.runtime,
      conditions: plan.conditions,
      agents: plan.agents.map(agent => ({
        adapter: agent.adapter,
        model: agent.model,
        identity: agent.identity,
      })),
      reports: [
        {
          schemaVersion: 1,
          generatedAt: now,
          ok: true,
          request: {
            backends: plan.stacks.map(stack => stack.id),
            track: plan.definition.track,
            levels: plan.definition.levels,
            runIndex: 0,
            parallelism: plan.summary.parallelism,
            agentAdapter: plan.agents[0]!.adapter,
            packs: [],
            checks: [],
            image: buildImage,
            resultsDir: campaign,
            smoke: false,
          },
          summary: { passed: 0, failed: 0, warnings: 0 },
          checks: [],
        },
      ],
    },
  });
  const claim = claimNextAttempt(state, { now, admissionId: 'fixture' });
  assert.ok(claim.claim);
  const attempt = claim.claim.attempt;
  const output = join(campaign, claim.claim.output);
  mkdirSync(join(output, 'source'), { recursive: true });
  writeFileSync(join(output, 'source/app.js'), 'export const ready = true;\n');
  const progression = compileProgressionInput(
    dependencyRuntimeDefinition(plan.featureCatalog, plan.dependencyPolicy)
  );
  const initial = progressionEngine.initialize(progression.definition);
  const selection = progressionEngine.gradingSelection(initial);
  const selectionSha256 = 'a'.repeat(64);
  const graded = progressionEngine.recordResult(initial, {
    attemptId: 'fixture-grade',
    outcome: 'conclusive',
    selectionSha256,
    nodes: selection.nodeIds.map(id => ({
      id,
      checks: selection.checks
        .filter(check => check.nodeId === id)
        .map(check => ({ id: check.id, outcome: 'fail' as const })),
    })),
  });
  assert.equal(graded.phase, 'terminal');
  assert.equal(graded.nodes.catalog!.status, 'blocked');
  writeProgressionState(join(output, 'progression-state.json'), {
    progression,
    featureCatalogIdentity: plan.featureCatalog.identity,
    dependencyPolicyIdentity: plan.dependencyPolicy.identity,
    owner: campaignProgressionOwner(plan, attempt, { workspace: true }),
    state: graded,
  });
  const checks = selection.checks.map(check => ({ stableKey: check.id, points: check.points }));
  const max = checks.reduce((sum, check) => sum + check.points, 0);
  const bundle = writeArtifact(join(output, 'grading/bundle.json'), {
    kind: 'grade_bundle',
    id: 'fixture-grade',
    payload: {
      observation: 'scored',
      source: { sha256: hashDirectory(join(output, 'source')).sha256 },
      suites: {
        fixture: {
          features: [
            {
              id: 'fixture',
              setupEvidence: createCheckEvidence({
                status: 'passed',
                code: 'completed',
                phase: 'setup',
                startedAtMs: 1,
                completedAtMs: 2,
              }),
              criteria: checks.map(check => ({
                id: check.stableKey,
                stableKey: check.stableKey,
                points: check.points,
                evidence: createCheckEvidence({
                  status: 'failed',
                  code: 'test_result',
                  phase: 'assertion',
                  startedAtMs: 1,
                  completedAtMs: 2,
                }),
              })),
            },
          ],
        },
      },
      totals: { score: 0, max },
      selection: {
        sha256: selectionSha256,
        checks,
        attemptedChecks: checks.map(check => check.stableKey),
        reportedChecks: checks.map(check => check.stableKey),
        notRun: [],
      },
    },
  });
  const gradeOutcome = classifyBundle(bundle.payload);
  const status = liveProgressionStatus(graded);
  const run = writeArtifact(join(output, 'run.json'), {
    kind: 'benchmark_run',
    id: 'fixture-run',
    attempt: { id: 'fixture-run', parentId: attempt.id },
    identities: emptyArtifactIdentities({
      engine: plan.identities.engine,
      experiment: {
        id: plan.id,
        version: plan.version,
        sha256: plan.contentSha256,
        state: plan.state,
      },
      agentAdapter: plan.agents[0]!.identity,
      stackAdapter: plan.stacks[0]!,
    }),
    payload: {
      mode: attempt.mode,
      track: plan.definition.track,
      backend: attempt.stack,
      model: attempt.model,
      pricing: attempt.pricing,
      guidance: attempt.guidance,
      condition: attempt.condition,
      selectionRequest: plan.definition.selection,
      featureCatalog: attempt.featureCatalog,
      dependencyPolicy: attempt.dependencyPolicy,
      progressionOwner: campaignProgressionOwner(plan, attempt),
      progressionStatus: status,
      skills: attempt.skills,
      runtime: { buildImage },
      validation: {
        ladder: {
          policy: 'dependency-graph',
          requestedLevels: attempt.levels,
          completedLevels: [1],
          stoppedAfterLevel: null,
          blockedLevels: [],
        },
      },
      totals: {
        score: 0,
        max: status.score.uniqueChecks.availablePoints,
        costUsd: 0,
        costComplete: false,
      },
      levels: [
        {
          level: 1,
          graded: true,
          selection: { sha256: selectionSha256 },
          score: 0,
          max,
          repairs: 0,
          repair: {
            status: 'budget-exhausted',
            limit: 0,
            used: 0,
            stopReason: null,
            nodeRepairs: dependencyLevelRepairRecords(graded, 1),
          },
          outcome: gradeOutcome,
        },
      ],
      outcome: { kind: 'app_failure', phase: 'grading', reason: null },
    },
  });
  state = finishCampaignExecution(
    claim.state,
    claim.claim.executionId,
    { exitCode: 0, run: artifactPayload(run) },
    { now }
  );
  const invalid = claimNextAttempt(state, { now, admissionId: 'fixture' });
  assert.ok(invalid.claim);
  state = finishCampaignExecution(
    invalid.state,
    invalid.claim.executionId,
    { exitCode: 1, run: null },
    { now }
  );
  writeCampaignState(join(campaign, 'state.json'), plan, state);
  assert.throws(() => createCampaignUpload(campaign, 'fixture-suite'), /report/);
  generateCampaignReport(campaign);
  const before = readFileSync(join(campaign, 'report/report.json'));
  const payload = createCampaignUpload(campaign, 'fixture-suite');
  assert.equal(payload.attempts.length, 2);
  assert.equal(payload.gradingStatus, 'pending');
  assert.deepEqual(
    [payload.attempts[0]!.featuresPassed, payload.attempts[0]!.featuresTotal],
    [0, 2]
  );
  assert.equal(payload.attempts[0]!.costUsd, null);
  assert.equal(payload.attempts[0]!.costStatus, 'unknown');
  assert.equal(payload.attempts[1]!.featuresPassed, null);
  assert.equal(payload.attempts[1]!.excluded, true);
  assert.ok(payload.attempts[1]!.excludedReason);
  // An operator exclusion keeps the attempt in the upload, marked with its reason.
  const replaced = payload.attempts[0]!.attemptId;
  const withExclusion = createCampaignUpload(campaign, 'fixture-suite',
    new Map([[replaced, 'replaced by a later run']]));
  assert.equal(withExclusion.attempts.length, 2);
  assert.equal(withExclusion.attempts[0]!.excluded, true);
  assert.equal(withExclusion.attempts[0]!.excludedReason, 'excluded by operator: replaced by a later run');
  assert.equal(withExclusion.attempts[0]!.featuresPassed, payload.attempts[0]!.featuresPassed);
  assert.throws(() => createCampaignUpload(campaign, 'fixture-suite', new Map([['no-such-attempt', 'x']])),
    /unknown attempt/);
  assert.throws(() => createCampaignUpload(campaign, 'fixture-suite', new Map([[replaced, ' ']])), /reason/);
  assert.deepEqual(readFileSync(join(campaign, 'report/report.json')), before);
  const exported = join(root, 'upload.json');
  const cli = join(dirname(fileURLToPath(import.meta.url)), '../commands/campaign-cli.js');
  const result = spawnSync(
    process.execPath,
    [cli, 'publish', campaign, '--suite', 'fixture-suite', '--out', exported],
    { encoding: 'utf8' }
  );
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(readFileSync(exported, 'utf8')), payload);
  await assert.rejects(
    publishCampaign(campaign, { suite: 'fixture-suite', output: exported }),
    /exist/i
  );
  const requests: unknown[] = [];
  let responseStatus = 201;
  let responseVersion = 1;
  const server = createServer(async (request, response) => {
    assert.equal(request.url, '/api/uploads');
    assert.equal(request.headers.authorization, 'Bearer fixture-key');
    let body = '';
    for await (const chunk of request) body += String(chunk);
    requests.push(JSON.parse(body));
    response.writeHead(responseStatus, { 'Content-Type': 'application/json', Connection: 'close' });
    response.end(
      JSON.stringify({
        version: responseVersion,
        uploadId: 'fixture-upload',
        status: 'pending',
        duplicate: false,
        runs: 2,
        ignored: 'private response metadata',
      })
    );
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  process.env.STACKBENCH_UPLOAD_KEY = 'fixture-key';
  t.after(() => {
    delete process.env.STACKBENCH_UPLOAD_KEY;
  });
  const published = await publishCampaign(campaign, {
    suite: 'fixture-suite',
    to: `http://127.0.0.1:${address.port}`,
  });
  assert.deepEqual(published, {
    version: 1,
    uploadId: 'fixture-upload',
    status: 'pending',
    duplicate: false,
    runs: 2,
  });
  assert.deepEqual(requests, [payload]);
  responseStatus = 409;
  await assert.rejects(
    publishCampaign(campaign, { suite: 'fixture-suite', to: `http://127.0.0.1:${address.port}` }),
    /HTTP 409/
  );
  responseStatus = 201;
  responseVersion = 2;
  await assert.rejects(
    publishCampaign(campaign, { suite: 'fixture-suite', to: `http://127.0.0.1:${address.port}` }),
    /acknowledgement/
  );
  if (process.env.STACKBENCH_PUBLISH_CHECK_PAYLOAD) {
    writeFileSync(
      process.env.STACKBENCH_PUBLISH_CHECK_PAYLOAD,
      `${JSON.stringify(payload, null, 2)}\n`
    );
  }
  if (process.env.STACKBENCH_PUBLISH_CHECK_CAMPAIGN) {
    cpSync(campaign, process.env.STACKBENCH_PUBLISH_CHECK_CAMPAIGN, {
      recursive: true,
      force: false,
      errorOnExist: true,
    });
  }
  await assert.rejects(
    publishCampaign(campaign, { suite: 'fixture-suite', to: 'http://example.com' }),
    /HTTPS/
  );
  state.attempts[1]!.executions[0]!.reason = 'Changed exclusion evidence';
  writeCampaignState(join(campaign, 'state.json'), plan, state);
  assert.throws(() => createCampaignUpload(campaign, 'fixture-suite'), /report/);
});
