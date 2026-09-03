import assert from 'node:assert/strict';
import { appendFileSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, utimesSync,
  watch, writeFileSync } from 'node:fs';
import { request as httpRequest } from 'node:http';
import type { Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { EventEmitter } from 'node:events';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution }
  from '../src/campaigns/campaign-scheduler.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { attemptChecks, attemptLogSlice, attemptPackage, campaignProgression, campaignSheet,
  overviewSummary } from '../dashboard/dashboard-views.js';
import { parseRunProgress,
  discoverCampaigns, discoverPlans, readCampaignArtifactBody, readJsonLines,
  resolveCampaignArtifact, summarizeCampaign,
} from '../dashboard/dashboard-model.js';
import { campaignFacts, firstGradeAbort, inspectCampaignSummary }
  from '../src/campaigns/campaign-inspection.js';
import { attemptExcluded } from '../dashboard/public/metrics.js';
import { createDashboardServer, parseDashboardArgs } from '../dashboard/dashboard-server.js';
import type { DashboardOperation, LaunchInput } from '../dashboard/dashboard-server.js';
import { sha256 } from '../src/evidence/provenance.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { writeProgressionState } from '../src/progression/progression-state.js';
import { attemptPage } from '../dashboard/public/views/attempt.js';
import { campaignPage } from '../dashboard/public/views/campaign.js';
import { campaignsPage } from '../dashboard/public/views/campaigns.js';
import { afterRun, plansPage, runName, topbar } from '../dashboard/public/views/plans.js';

import { DEPENDENCY_CAMPAIGN, EXAMPLE_CAMPAIGN, FIXTURE_CAMPAIGNS,
  dependencyProgressionEvidence, writeCampaign, writeDependencyResults, writeFixtureResults,
  writePlanFixtures, writeRunEvidence } from './fixtures/dashboard-fixture.js';

async function listenOrigin(server: Server): Promise<string> {
  await new Promise<void>((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  return `http://127.0.0.1:${(address as AddressInfo).port}`;
}

test('dashboard run progress reports only completed grades while the next repair is underway', () => {
  const progress = parseRunProgress(`
=== postgres-l1-first (postgres) ===
  TOTAL      ... 31/58
--- repair 1/3 ---
=== postgres-l1-fix1 (postgres) ===
  TOTAL      ... 41/58
--- repair 2/3 ---
=== postgres-l1-fix2 (postgres) ===
`, { repairs: 3 });
  assert.deepEqual(progress.firstScore, { score: 31, max: 58 });
  assert.deepEqual(progress.latestScore, { score: 41, max: 58 });
  assert.equal(progress.completedGrades, 2);
  assert.equal(progress.level, 1);
  assert.deepEqual(progress.repair, { round: 2, budget: 3 });
  assert.equal(progress.phase, 'Grading L1 after repair 2 of 3');
});

test('dashboard operation feed ignores only a truncated final write', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-feed-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const path = join(root, 'operations.jsonl');
  writeFileSync(path, '{"id":"complete"}\n{"id":"partial"');
  assert.deepEqual(readJsonLines(path), [{ id: 'complete' }]);
  writeFileSync(path, '{broken}\n{"id":"complete"}\n');
  assert.throws(() => readJsonLines(path), /line 1/);
});

test('dashboard follows the actual level when a completed L1 advances to the first L2 grade', () => {
  const progress = parseRunProgress(`
=== spacetime-l1 (spacetime) ===
  TOTAL      ... 52/58
--- repair 1/10 ---
=== spacetime-l1-fix1 (spacetime) ===
  TOTAL      ... 58/58
=== spacetime-l2 (spacetime) ===
  TOTAL      ... 52/59
--- repair 1/10 ---
`, { repairs: 10 });
  assert.equal(progress.level, 2);
  assert.equal(progress.phase, 'Repairing L2 · round 1 of 10');
  assert.deepEqual(progress.firstScore, { score: 52, max: 58 });
  assert.deepEqual(progress.latestScore, { score: 52, max: 59 });
});

test('dashboard reports a dependency repair by feature instead of the level session count', () => {
  const progress = parseRunProgress(`
=== mongodb-l3-first (mongodb) ===
  TOTAL      ... 26/43
--- feature repair 1/1: Customer recommendations ---
=== mongodb-l3-fix3 (mongodb) ===
`, { repairs: 3 });
  assert.equal(progress.phase,
    'Grading L3 after repair 1 of 1 for Customer recommendations');
  assert.deepEqual(progress.repair, { round: 1, budget: 1 });
});

test('dashboard calls dependency graph position depth', () => {
  const progress = parseRunProgress(`
=== mongodb-l2-first (mongodb) ===
`, { dependency: true });
  assert.equal(progress.phase, 'Grading the first depth 2 build');
});

test('an aborted first grade is classified, not treated as a scored zero', () => {
  // A readiness failure stops the grade before any suite runs, so 0/58 must
  // read as an abort with its reason.
  assert.deepEqual(firstGradeAbort({ score: 0, max: 58, outcome: { kind: 'app_failure',
    phase: 'application-readiness', reason: 'application did not respond' } }),
  { phase: 'application-readiness', reason: 'application did not respond' });
  // A graded first build — even a zero — is a score, not an abort.
  assert.equal(firstGradeAbort({ score: 0, max: 58,
    outcome: { kind: 'app_failure', phase: 'grading', reason: null } }), null);
  assert.equal(firstGradeAbort({ score: 31, max: 58,
    outcome: { kind: 'passed', phase: 'grading' } }), null);
  assert.equal(firstGradeAbort(null), null);
  assert.equal(firstGradeAbort({ score: 31, max: 58 }), null);
});

test('campaign facts surface the identity an operator otherwise reads plan.json for', () => {
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const facts = campaignFacts(plan);
  assert.deepEqual(facts.agents, plan.agents.map(agent => ({ adapter: agent.adapter,
    version: agent.adapterVersion, model: agent.model })));
  assert.deepEqual(facts.recipes, plan.attempts[0]?.condition.requested.levels.map(level => ({
    level: level.level, id: level.recipe?.id ?? null, version: level.recipe?.version ?? null,
  })));
  assert.deepEqual(facts.runtime, {
    controllerImage: plan.definition.runtime.controllerImage,
    buildImage: plan.definition.runtime.buildImage,
  });
  assert.equal(facts.grading.status, 'pending');
  assert.equal(Object.hasOwn(plan.bindings[0]!, 'qualification'), false);
});

test('dashboard reports dependency work from the validated persisted state', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-dependency-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const plan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  const now = '2026-08-25T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const claim = claimed.claim;
  const state = claimed.state;
  const attemptState = state.attempts.find(attempt =>
    attempt.executions.at(-1)?.id === claim.executionId);
  assert.ok(attemptState);
  const attemptPlan = attemptState.plan;
  const outputRelative = claim.output;
  const output = join(root, outputRelative);
  mkdirSync(join(output, 'source'), { recursive: true });
  writeCampaign(root, plan, state);
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  const owner = { schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    attempt: { id: attemptPlan.id, track: plan.definition.track, stack: attemptPlan.stack,
      agentAdapter: attemptPlan.agentAdapter, model: attemptPlan.model,
      conditionSha256: attemptPlan.condition.sha256 },
    workspace: { appDirectory: 'source' } };
  assert(plan.featureCatalog && plan.dependencyPolicy);
  writeProgressionState(join(output, 'progression-state.json'), { progression,
    featureCatalogIdentity: plan.featureCatalog.identity,
    dependencyPolicyIdentity: plan.dependencyPolicy.identity,
    owner, state: progressionEngine.initialize(progression.definition) });

  const summary = summarizeCampaign(root, { includePackage: true });
  assert.equal(summary.mode, 'dependency');
  const firstAttempt = summary.attempts[0];
  assert.ok(firstAttempt?.dependency);
  assert.deepEqual(firstAttempt.dependency.activeDepths, [1]);
  assert.deepEqual(firstAttempt.dependency.history,
    { firstTryPercentage: 0, repairAttempts: 0 });
  assert(firstAttempt.dependency.work.current.some(node => node.id === 'accounts'));
  assert.deepEqual(firstAttempt.dependency.attempts,
    { total: 0, maxRemaining: 0, features: [] });
  assert.ok(summary.package);
  assert(summary.package.executions[0]?.artifacts
    .some(item => item.path.endsWith('/progression-state.json')));
});

test('a prepared attempt is waiting rather than finished', () => {
  const progress = parseRunProgress('', { repairs: 10, running: false, status: 'pending' });
  assert.equal(progress.phase, 'Waiting to start');
  assert.equal(progress.completedGrades, 0);
  assert.equal(progress.latestScore, null);
});

test('dashboard CLI is deliberately loopback-only', () => {
  const parsed = parseDashboardArgs(['node', 'dashboard', '--port', '7444', '--host', 'localhost'], {});
  assert.equal(parsed.port, 7444);
  assert.equal(parsed.host, 'localhost');
  const container = parseDashboardArgs(['node', 'dashboard', '--host', '0.0.0.0',
    '--allow-container-bind'], { STACK_BENCH_APPLIANCE: '1' });
  assert.equal(container.host, '0.0.0.0');
  assert.equal(container.allowContainerBind, true);
  assert.throws(() => parseDashboardArgs(['node', 'dashboard', '--host', '0.0.0.0'], {}),
    /loopback/);
  assert.throws(() => parseDashboardArgs(['node', 'dashboard', '--host', '0.0.0.0',
    '--allow-container-bind'], {}), /loopback/);
  assert.throws(() => parseDashboardArgs(['node', 'dashboard', '--port', '70000'], {}),
    /port/);
});

test('dashboard marks a current-schema campaign as interrupted when its controller stops', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-interrupted-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  writeCampaign(root, plan, claimed.state);
  const output = join(root, claimed.claim.output);
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'process.stdout.log'),
    '=== postgres-l1-first (postgres) ===\n  TOTAL ... 31/58\n--- repair 1/3 ---\n');

  const summary = summarizeCampaign(root);
  assert.equal(summary.status, 'running');
  const activeAttempt = summary.attempts[0];
  assert.ok(activeAttempt);
  assert.equal(activeAttempt.progress.phase, 'Repairing L1 · round 1 of 3');
  assert.deepEqual(activeAttempt.progress.latestScore, { score: 31, max: 58 });

  const interrupted = summarizeCampaign(root, { controllerActive: () => false });
  assert.equal(interrupted.status, 'attention-required');
  assert.equal(interrupted.interrupted, true);
  assert.equal(interrupted.summary.running, 0);
  assert.equal(interrupted.summary.interrupted, 1);
  const interruptedAttempt = interrupted.attempts[0];
  assert.ok(interruptedAttempt?.execution);
  assert.equal(interruptedAttempt.status, 'interrupted');
  assert.equal(interruptedAttempt.execution.status, 'interrupted');
  assert.equal(interruptedAttempt.progress.phase, 'Controller stopped before completion');
});

test('dashboard keeps frozen campaign evidence readable after the controller is upgraded', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-frozen-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const currentPlan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const state = createCampaignState(currentPlan, { now: '2026-08-18T12:00:00.000Z' });
  const frozenPlan = structuredClone(currentPlan);
  frozenPlan.identities.engine.sha256 = 'f'.repeat(64);
  frozenPlan.contentSha256 = sha256(canonicalDefinitionJson({
    campaignSchemaVersion: frozenPlan.campaignSchemaVersion,
    definition: frozenPlan.definition,
    engine: frozenPlan.identities.engine,
    bindings: frozenPlan.bindings,
    stacks: frozenPlan.stacks,
    agents: frozenPlan.agents,
    conditions: frozenPlan.conditions,
  }));
  state.campaignSha256 = frozenPlan.contentSha256;
  writeArtifact(join(root, 'plan.json'), { kind: 'campaign_plan', id: `${frozenPlan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: frozenPlan });
  writeArtifact(join(root, 'state.json'), { kind: 'campaign_state', id: `${frozenPlan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });

  const summary = summarizeCampaign(root);
  assert.equal(summary.id, frozenPlan.id);
  assert.equal(summary.status, 'prepared');
  assert.equal(summary.attempts.length, frozenPlan.attempts.length);
});

test('dashboard rejects a run artifact that does not belong to its frozen attempt', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-run-identity-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  writeCampaign(root, plan, claimed.state);
  const output = join(root, claimed.claim.output);
  mkdirSync(output, { recursive: true });
  const attemptPlan = claimed.claim.attempt;
  writeArtifact(join(output, 'run.json'), { kind: 'benchmark_run', id: 'other-run',
    attempt: { id: 'other-run', parentId: 'other-attempt' },
    identities: emptyArtifactIdentities(), payload: {
      mode: attemptPlan.mode, track: plan.definition.track, backend: attemptPlan.stack,
      model: attemptPlan.model, pricing: attemptPlan.pricing, guidance: attemptPlan.guidance,
      condition: attemptPlan.condition, selectionRequest: plan.definition.selection,
      featureCatalog: attemptPlan.featureCatalog ?? null,
      dependencyPolicy: attemptPlan.dependencyPolicy ?? null,
      progressionOwner: null, skills: attemptPlan.skills, levels: [],
      runtime: { buildImage: plan.definition.runtime.buildImage },
      totals: { costUsd: 0, costComplete: true }, outcome: { kind: 'passed' },
    } });

  const attempt = summarizeCampaign(root).attempts[0];
  assert.match(attempt?.result?.unreadable ?? '', /attempt\.parentId/);
  const inspection = inspectCampaignSummary(root);
  const inspected = inspection.attempts[0];
  assert.equal(inspection.schemaVersion, 1);
  assert.match(inspected?.result?.unreadable ?? '', /attempt\.parentId/);
  assert.equal(inspected?.artifacts?.run, `${claimed.claim.output}/run.json`);
});

test('dashboard overview defers historical evidence validation until the sheet is opened', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-history-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const key = 'historical-run';
  const campaign = join(resultsRoot, 'campaigns', key);
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  let state = createCampaignState(plan, { now: '2026-08-18T12:00:00.000Z' });
  let firstClaim = null;
  for (;;) {
    const claimed = claimNextAttempt(state,
      { now: '2026-08-18T12:00:00.000Z', admissionId: 'admission-1' });
    if (!claimed.claim) break;
    firstClaim ??= claimed.claim;
    state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
      { exitCode: 0, run: { outcome: { kind: 'passed' } } },
      { now: '2026-08-18T12:00:01.000Z' });
  }
  assert.ok(firstClaim);
  writeCampaign(campaign, plan, state);
  const output = join(campaign, firstClaim.output);
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'run.json'), '{"not":"a benchmark run"}\n');

  const overview = discoverCampaigns(join(resultsRoot, 'campaigns'))[0];
  assert.ok(overview && overview.status !== 'unreadable');
  assert.equal(overview.status, 'completed');
  assert.equal(overview.attempts.length, 0);

  const sheet = campaignSheet(resultsRoot, key);
  const excluded = sheet.stacks.flatMap(stack => stack.attempts)
    .map(attempt => attempt.excluded);
  assert.ok(excluded.includes('result could not be read'));
});

test('dashboard keeps the recorded invalidation reason when the result is unreadable', () => {
  assert.equal(attemptExcluded({
    id: 'failed-attempt', stack: 'mongodb', status: 'invalid',
    execution: { outcome: 'harness_failure', reason: 'disk filled while saving the run' },
    result: { unreadable: 'partial run is invalid' }, dependency: null,
  }), 'disk filled while saving the run');
});

test('the attempt package exposes the evidence but not arbitrary campaign files', async t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-package-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const campaign = join(resultsRoot, 'campaigns', 'evidence-run');
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } },
    { now: '2026-08-18T12:00:01.000Z' });
  const outputRelative = claimed.claim.output;
  writeCampaign(campaign, plan, state);
  const output = join(campaign, outputRelative);
  const media = join(output, 'grading', 'failure-media');
  mkdirSync(media, { recursive: true });
  writeFileSync(join(output, 'process.stdout.log'), 'authorization: Bearer secret-token-value\n'
    + 'ANTHROPIC_API_KEY=provider-secret\nCLAUDE_CODE_OAUTH_TOKEN=oauth-secret\n'
    + '{"apiKey":"json-secret"}\nresult ok\n');
  writeFileSync(join(output, 'run.json'), '{"result":"ok"}\n');
  writeArtifact(join(output, 'process.json'), { kind: 'campaign_process', id: 'execution-process',
    payload: { schemaVersion: 1, executionId: claimed.claim.executionId, runIndex: 0,
      exitCode: 1, signal: null, timedOut: false, streams: null } });
  writeFileSync(join(media, 'failed-check.png'), Buffer.from('89504e470d0a1a0a', 'hex'));
  mkdirSync(join(campaign, 'admissions'), { recursive: true });
  writeFileSync(join(campaign, 'admissions', 'private.json'), '{"token":"do-not-serve"}\n');

  const attemptId = claimed.claim.attempt.id;
  const evidence = attemptPackage(resultsRoot, 'evidence-run', attemptId);
  const packageExecution = evidence.executions[0];
  assert.ok(packageExecution);
  assert.equal(evidence.executions.length, 1);
  assert.equal(packageExecution.truncated, false);
  const initialVisual = packageExecution.visuals[0];
  assert.ok(initialVisual);
  assert.equal(initialVisual.path,
    `${outputRelative}/grading/failure-media/failed-check.png`);
  const log = packageExecution.artifacts.find(item => item.path.endsWith('process.stdout.log'));
  assert.ok(log);
  const process = packageExecution.artifacts.find(item => item.path.endsWith('process.json'));
  assert.ok(process);
  const servedLog = readCampaignArtifactBody(
    resolveCampaignArtifact(resultsRoot, 'evidence-run', log.id)).toString();
  assert.match(servedLog, /\[redacted credential\]/);
  assert.doesNotMatch(servedLog, /secret-token-value|provider-secret|oauth-secret|json-secret/);
  assert.throws(() => resolveCampaignArtifact(resultsRoot, 'evidence-run',
    Buffer.from('admissions/private.json').toString('base64url')), /not available/);
  assert.throws(() => resolveCampaignArtifact(resultsRoot, 'evidence-run',
    Buffer.from('../outside.json').toString('base64url')), /not available|outside/);

  const feed = { append() {}, list() { return []; } };
  const { server } = createDashboardServer({ resultsRoot, plansRoot: join(resultsRoot, 'plans'),
    allowLaunch: false, token: 'package-token', feed, plans: () => [] });
  const origin = await listenOrigin(server);
  t.after(() => server.close());
  const visual = packageExecution.visuals[0];
  assert.ok(visual);
  const visualResponse = await fetch(`${origin}/api/campaigns/evidence-run/artifacts/${visual.id}`);
  assert.equal(visualResponse.status, 200);
  assert.equal(visualResponse.headers.get('content-type'), 'image/png');
  assert.deepEqual(Buffer.from(await visualResponse.arrayBuffer()), Buffer.from('89504e470d0a1a0a', 'hex'));
  const logResponse = await fetch(`${origin}/api/campaigns/evidence-run/artifacts/${log.id}`);
  assert.equal(logResponse.status, 200);
  assert.match(await logResponse.text(), /\[redacted credential\]/);
  const processResponse = await fetch(`${origin}/api/campaigns/evidence-run/artifacts/${process.id}`);
  assert.equal(processResponse.status, 200);
  assert.match(await processResponse.text(), /"timedOut": false/);
  const forbidden = Buffer.from('admissions/private.json').toString('base64url');
  assert.equal((await fetch(`${origin}/api/campaigns/evidence-run/artifacts/${forbidden}`)).status, 404);
});

test('dashboard serves real state and protects campaign launch with a separate operator secret', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-'));
  const plansRoot = join(root, 'plans');
  mkdirSync(plansRoot, { recursive: true });
  const events: DashboardOperation[] = [];
  const launches: LaunchInput[] = [];
  const feed = {
    append(event: DashboardOperation) { events.push(structuredClone(event)); },
    list() {
      const latest = new Map<string, DashboardOperation>();
      for (const event of events) latest.set(event.id, { ...(latest.get(event.id) ?? {}), ...event });
      return [...latest.values()];
    },
  };
  const storedPlan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  const frozenPlan = { id: storedPlan.id, version: storedPlan.version, title: storedPlan.title,
    state: 'frozen', mode: 'dependency', track: storedPlan.definition.track,
    levels: storedPlan.definition.levels, stacks: storedPlan.stacks.map(stack => stack.id),
    attempts: storedPlan.summary.attempts, parallelism: storedPlan.summary.parallelism,
    budgets: storedPlan.definition.budgets, sha256: storedPlan.contentSha256,
    file: 'ecommerce-progression-reference.json' };
  const campaignDirectory = join(root, 'campaigns', 'prepared-run');
  const now = '2026-08-25T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(storedPlan, { now }),
    { now, admissionId: 'old' });
  assert.ok(claimed.claim);
  const storedState = finishCampaignExecution(claimed.state, claimed.claim.executionId, {
    exitCode: 1,
    run: { outcome: { kind: 'harness_failure', reason: 'controller stopped' } },
    retryAuthority: { transient: true, recoveryClean: true, budgetKnown: true,
      cause: 'controller stopped' },
  }, { now: '2026-08-25T12:00:01.000Z', retries: 1, retryOn: ['harness_failure'] });
  writeCampaign(campaignDirectory, storedPlan, storedState);
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot, allowLaunch: true,
    token: 'test-session-token', controlSecret: 'test-control-secret-value-1234567890',
    feed, plans: () => [frozenPlan],
    launch(input) { launches.push(input); const child = Object.assign(new EventEmitter(), { pid: 1234 });
      return child; } });
  const origin = await listenOrigin(server);
  t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });

  const page = await fetch(origin);
  assert.equal(page.status, 200);
  const pageHtml = await page.text();
  assert.match(pageHtml, /<title>Stack Bench<\/title>/);
  assert.match(pageHtml, /src="\/app\.js"/);
  assert.doesNotMatch(pageHtml, /Control Room/);
  assert.doesNotMatch(pageHtml, /Controller activity/);
  assert.doesNotMatch(pageHtml, /See every run/);
  const brand = await fetch(`${origin}/spacetimedb-mark.svg`);
  assert.equal(brand.status, 200);
  assert.equal(brand.headers.get('content-type'), 'image/svg+xml');
  assert.match(await brand.text(), /viewBox="0 0 35 32"/);
  const overview = await (await fetch(`${origin}/api/overview`)).json() as {
    canStart: boolean;
    csrfToken: string;
    campaigns: Array<{ key: string }>;
  };
  assert.equal(overview.canStart, true);
  assert.equal(overview.csrfToken, 'test-session-token');
  assert.doesNotMatch(JSON.stringify(overview), /test-control-secret-value/);
  assert.deepEqual(overview.campaigns.map(campaign => campaign.key), ['prepared-run']);
  const served = await (await fetch(`${origin}/api/plans`)).json() as Array<{ id: string }>;
  assert.deepEqual(served.map(plan => plan.id), [frozenPlan.id]);

  const reboundStatus = await new Promise((resolveStatus, reject) => {
    const rebound = httpRequest(`${origin}/api/overview`, { headers: { host: 'attacker.invalid' } },
      response => { response.resume(); response.once('end', () => resolveStatus(response.statusCode)); });
    rebound.once('error', reject);
    rebound.end();
  });
  assert.equal(reboundStatus, 421);

  const rejected = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(rejected.status, 403);
  assert.equal(launches.length, 0);

  const wrongOperator = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token',
      'x-stack-bench-control-secret': 'wrong-control-secret-value-1234567890' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(wrongOperator.status, 403);
  assert.equal(launches.length, 0);

  const accepted = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token',
      'x-stack-bench-control-secret': 'test-control-secret-value-1234567890' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(accepted.status, 202);
  assert.equal(launches.length, 1);
  const firstLaunch = launches[0];
  const firstEvent = feed.list()[0];
  assert.ok(firstLaunch && firstEvent);
  assert.equal(firstLaunch.plan.id, frozenPlan.id);
  assert.equal(firstLaunch.output, join(root, 'campaigns', 'meeting-run-1'));
  assert.equal(firstEvent.status, 'running');
  assert.equal(firstEvent.pid, 1234);
  assert.equal(firstEvent.campaignSha256, frozenPlan.sha256);
  const duplicateStart = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token',
      'x-stack-bench-control-secret': 'test-control-secret-value-1234567890' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(duplicateStart.status, 409);
  assert.equal(launches.length, 1);

  const resumed = await fetch(`${origin}/api/campaigns/prepared-run/resume`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token',
      'x-stack-bench-control-secret': 'test-control-secret-value-1234567890' }, body: '{}' });
  assert.equal(resumed.status, 202);
  assert.equal(launches.length, 2);
  assert.equal(launches[1]?.output, campaignDirectory);
  assert.equal(((await resumed.json()) as { type?: string }).type, 'campaign.resume');
  const duplicate = await fetch(`${origin}/api/campaigns/prepared-run/resume`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token',
      'x-stack-bench-control-secret': 'test-control-secret-value-1234567890' }, body: '{}' });
  assert.equal(duplicate.status, 409);
  assert.equal(launches.length, 2);
});

test('the run form sends the plan, the run name and the operator secret the route requires',
  async t => {
    const root = mkdtempSync(join(tmpdir(), 'stack-bench-run-form-'));
    const plansRoot = join(root, 'plans');
    writePlanFixtures(plansRoot);
    const launches: LaunchInput[] = [];
    const feed = { append() {}, list() { return []; } };
    const { server } = createDashboardServer({ resultsRoot: root, plansRoot, allowLaunch: true,
      token: 'form-token', controlSecret: 'form-control-secret-value-1234567890', feed,
      plans: () => discoverPlans(plansRoot),
      launch(input) {
        launches.push(input);
        return Object.assign(new EventEmitter(), { pid: 77 });
      } });
    const origin = await listenOrigin(server);
    t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });
    const plan = discoverPlans(plansRoot).find(item => item.state === 'frozen');
    assert.ok(plan);
    // The form's own prefill, held to the name the route accepts.
    const outputName = runName(plan.id, new Date('2026-09-02T15:04:00'));
    assert.match(outputName, /^[a-z0-9][a-z0-9.-]{2,119}$/);
    const send = (secret: string): Promise<Response> => fetch(`${origin}/api/campaigns`,
      { method: 'POST',
        headers: { origin, 'content-type': 'application/json', 'x-stack-bench-token': 'form-token',
          'x-stack-bench-control-secret': secret },
        body: JSON.stringify({ planId: plan.id, outputName }) });

    const refused = await send('wrong-control-secret-value-1234567890');
    assert.equal(refused.status, 403);
    assert.equal(launches.length, 0);
    const typed = { planId: plan.id, outputName, secret: 'typed-secret', error: '' };
    const refusedError = ((await refused.json()) as { error: string }).error;
    assert.deepEqual(afterRun(typed, refused.status, refusedError),
      { planId: plan.id, outputName, secret: '', error: 'The run request is not authorized.' });
    assert.equal(afterRun(typed, 409, 'That run output already exists.').secret, 'typed-secret');

    const accepted = await send('form-control-secret-value-1234567890');
    assert.equal(accepted.status, 202);
    assert.equal(launches.length, 1);
    assert.equal(launches[0]?.plan.id, plan.id);
    assert.equal(launches[0]?.output, join(root, 'campaigns', outputName));
  });

test('host development mode is read-only even with a valid browser request', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-readonly-'));
  mkdirSync(join(root, 'plans'), { recursive: true });
  const feed = { append() {}, list() { return []; } };
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: false, token: 'readonly-token', feed, plans: () => [] });
  const origin = await listenOrigin(server);
  t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });
  const response = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json', 'x-stack-bench-token': 'readonly-token' },
    body: JSON.stringify({ planId: 'anything', outputName: 'meeting-run-2' }) });
  assert.equal(response.status, 503);
});

test('the overview stays a summary and the sheet stays one campaign at appliance scale', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-scale-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  writeFixtureResults(resultsRoot);
  const campaignsRoot = join(resultsRoot, 'campaigns');
  const controllerActive = (): boolean => true;

  const overview = overviewSummary(campaignsRoot, { controllerActive });
  assert.equal(overview.length, FIXTURE_CAMPAIGNS);
  const first = overview[0];
  assert.ok(first && 'scores' in first);
  assert.equal(Object.keys(first.scores).length, 3);
  assert.equal(first.attempts.total, 9);
  assert.ok(Object.values(first.scores).every(score => score !== null));
  assert.equal(JSON.stringify(overview).includes('process.stdout.log'), false);
  const overviewBytes = Buffer.byteLength(JSON.stringify(overview));
  assert.ok(overviewBytes < 12 * 1024, `overview payload is ${overviewBytes} bytes`);
  const startedAt = performance.now();
  overviewSummary(campaignsRoot, { controllerActive });
  const warmMs = performance.now() - startedAt;
  assert.ok(warmMs < 150, `warm overview took ${warmMs.toFixed(1)}ms`);

  const sheet = campaignSheet(resultsRoot, 'fixture-run-0', { controllerActive });
  assert.equal(sheet.mode, 'sequential');
  assert.equal(sheet.facts.repairBudget, 3);
  assert.equal(sheet.facts.timeLimitMinutes, 240);
  assert.equal(sheet.stacks.length, 3);
  const stack = sheet.stacks[0];
  assert.ok(stack);
  assert.equal(stack.n, 3);
  assert.ok(stack.score !== null && stack.unaided !== null);
  assert.ok(stack.unaided < stack.score);
  assert.deepEqual(stack.levels?.map(level => level.level), [1]);
  assert.equal(stack.questlines, null);
  assert.equal(stack.attempts.length, 3);
  assert.ok(stack.climb.length >= 2);
  const sheetBytes = Buffer.byteLength(JSON.stringify(sheet));
  assert.ok(sheetBytes < 60 * 1024, `campaign sheet is ${sheetBytes} bytes`);
});

test('a liveness probe that cannot answer still reads the campaign', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-liveness-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  writeFixtureResults(resultsRoot, { running: true });
  const controllerActive = (): boolean => {
    throw new Error('failed to connect to the docker API at unix:///var/run/docker.sock');
  };

  const campaign = overviewSummary(join(resultsRoot, 'campaigns'), { controllerActive })
    .find(entry => entry.key === 'fixture-run-0');
  assert.ok(campaign && 'scores' in campaign);
  assert.equal(campaign.status, 'running');
  assert.equal(campaignSheet(resultsRoot, 'fixture-run-0', { controllerActive }).status, 'running');
});

test('the overview caches a running campaign until its evidence changes', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-cache-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  writeFixtureResults(resultsRoot, { running: true });
  const campaignsRoot = join(resultsRoot, 'campaigns');
  const controllerActive = (): boolean => true;

  const first = overviewSummary(campaignsRoot, { controllerActive })
    .find(campaign => campaign.key === 'fixture-run-0');
  assert.ok(first && 'scores' in first && first.status === 'running');
  const cached = overviewSummary(campaignsRoot, { controllerActive })
    .find(campaign => campaign.key === 'fixture-run-0');
  assert.equal(cached, first);

  const attempts = join(campaignsRoot, 'fixture-run-0', 'attempts');
  const attempt = readdirSync(attempts)[0];
  assert.ok(attempt);
  const run = join(attempts, attempt, 'execution-1', 'run.json');
  const later = new Date(Date.now() + 5000);
  utimesSync(run, later, later);
  const refreshed = overviewSummary(campaignsRoot, { controllerActive })
    .find(campaign => campaign.key === 'fixture-run-0');
  assert.ok(refreshed);
  assert.notEqual(refreshed, first);
  assert.deepEqual(refreshed, first);
});

test('attempt evidence is fetched per attempt, not per campaign', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-attempt-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } }, { now });
  const directory = join(resultsRoot, 'campaigns', 'attempt-run');
  writeCampaign(directory, plan, state);
  const output = join(directory, claimed.claim.output);
  mkdirSync(output, { recursive: true });
  writeRunEvidence(output, plan, claimed.claim.attempt, 1);
  writeFileSync(join(output, 'process.stdout.log'),
    `authorization: Bearer secret-token-value\n${'log line\n'.repeat(64)}`);
  const attemptId = claimed.claim.attempt.id;

  const checks = attemptChecks(resultsRoot, 'attempt-run', attemptId);
  assert.equal(checks.stack, claimed.claim.attempt.stack);
  assert.deepEqual(checks.grades.map(grade => grade.id), ['grading']);
  assert.ok(checks.checks.length > 0);
  assert.ok(checks.checks.every(check => check.outcome === 'pass'));
  assert.ok(checks.checks.every(check => check.history.length === 1));
  assert.equal(checks.checks.some(check => check.regressed), false);

  const evidence = attemptPackage(resultsRoot, 'attempt-run', attemptId);
  assert.equal(evidence.executions.length, 1);
  const paths = evidence.executions[0]?.artifacts.map(item => item.path) ?? [];
  assert.ok(paths.some(path => path.endsWith('/run.json')));
  assert.ok(paths.some(path => path.endsWith('/grading/bundle.json')));
  assert.equal(paths.some(path => path.includes('/source/')), false);

  const head = attemptLogSlice(resultsRoot, 'attempt-run', attemptId, 0);
  assert.match(head.text, /\[redacted credential\]/);
  assert.doesNotMatch(head.text, /secret-token-value/);
  assert.equal(head.offset, head.size);
  const tail = attemptLogSlice(resultsRoot, 'attempt-run', attemptId, head.offset);
  assert.equal(tail.text, '');
  assert.equal(tail.offset, head.size);
});

test('the sheet reports dependency submodes and questlines in definition order', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-sheet-dependency-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const plan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  const now = '2026-08-25T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const directory = join(resultsRoot, 'campaigns', 'dependency-run');
  const output = join(directory, claimed.claim.output);
  mkdirSync(join(output, 'source'), { recursive: true });
  writeCampaign(directory, plan, claimed.state);
  assert(plan.featureCatalog && plan.dependencyPolicy);
  const attemptPlan = claimed.claim.attempt;
  writeProgressionState(join(output, 'progression-state.json'), {
    progression: compileProgressionInput(dependencyRuntimeDefinition(
      plan.featureCatalog, plan.dependencyPolicy)),
    featureCatalogIdentity: plan.featureCatalog.identity,
    dependencyPolicyIdentity: plan.dependencyPolicy.identity,
    owner: { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attemptPlan.id, track: plan.definition.track, stack: attemptPlan.stack,
        agentAdapter: attemptPlan.agentAdapter, model: attemptPlan.model,
        conditionSha256: attemptPlan.condition.sha256 },
      workspace: { appDirectory: 'source' } },
    state: progressionEngine.initialize(compileProgressionInput(dependencyRuntimeDefinition(
      plan.featureCatalog, plan.dependencyPolicy)).definition) });

  const sheet = campaignSheet(resultsRoot, 'dependency-run', { controllerActive: () => true });
  assert.equal(sheet.mode, 'dependency');
  assert.equal(sheet.facts.workSelection, 'progressive');
  assert.equal(sheet.facts.repairSelection, 'feature');
  assert.equal(sheet.facts.repairBudget, 0);
  assert.equal(sheet.facts.guidance, attemptPlan.guidance);
  const stack = sheet.stacks.find(item => item.questlines !== null);
  assert.ok(stack?.questlines);
  assert.equal(stack.levels, null);
  assert.ok(stack.questlines.length > 0);
  const questline = stack.questlines[0];
  assert.ok(questline);
  assert.ok(questline.nodes.length > 0);
  assert.ok(questline.nodes.every(node => typeof node.status === 'string'));
  assert.equal(stack.repairs.budget, 0);
  assert.equal(stack.regressions, 0);
  const bytes = Buffer.byteLength(JSON.stringify(sheet));
  assert.ok(bytes < 60 * 1024, `dependency sheet is ${bytes} bytes`);
});

function writeSingleAttemptCampaign(resultsRoot: string, key: string): string {
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } }, { now });
  const directory = join(resultsRoot, 'campaigns', key);
  writeCampaign(directory, plan, state);
  const output = join(directory, claimed.claim.output);
  mkdirSync(output, { recursive: true });
  writeRunEvidence(output, plan, claimed.claim.attempt, 1);
  writeFileSync(join(output, 'process.stdout.log'),
    `authorization: Bearer secret-token-value\n${'log line\n'.repeat(8)}`);
  return claimed.claim.attempt.id;
}

test('the sheet says a dependency campaign that stopped between executions can resume', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-resume-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const plan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  const now = '2026-08-25T12:00:00.000Z';
  const claimed = claimNextAttempt(createCampaignState(plan, { now }),
    { now, admissionId: 'admission-1' });
  assert.ok(claimed.claim);
  const stopped = finishCampaignExecution(claimed.state, claimed.claim.executionId, {
    exitCode: 1,
    run: { outcome: { kind: 'harness_failure', reason: 'controller stopped' } },
    retryAuthority: { transient: true, recoveryClean: true, budgetKnown: true,
      cause: 'controller stopped' },
  }, { now, retries: 1, retryOn: ['harness_failure'] });
  writeCampaign(join(resultsRoot, 'campaigns', 'prepared-run'), plan, stopped);
  const sheet = campaignSheet(resultsRoot, 'prepared-run', { controllerActive: () => false });
  assert.equal(sheet.status, 'prepared');
  assert.equal(sheet.executions, 1);
  assert.equal(sheet.resumable, true);
  // A campaign that has run nothing, and a sequential one, have nothing to resume.
  writeCampaign(join(resultsRoot, 'campaigns', 'fresh-run'), plan, createCampaignState(plan, { now }));
  assert.equal(campaignSheet(resultsRoot, 'fresh-run', { controllerActive: () => false })
    .resumable, false);
  writeFixtureResults(resultsRoot);
  assert.equal(campaignSheet(resultsRoot, 'fixture-run-0', { controllerActive: () => false })
    .resumable, false);
});

test('every view has a route, and a name that is not a campaign never reaches the evidence',
  async t => {
    const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-routes-'));
    const attemptId = writeSingleAttemptCampaign(root, 'route-run');
    const feed = { append() {}, list() { return []; } };
    const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
      allowLaunch: false, token: 'route-token', feed, plans: () => [] });
    const origin = await listenOrigin(server);
    t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });

    const overview = await fetch(`${origin}/api/overview`);
    assert.equal(overview.headers.get('cache-control'), 'no-store');
    const summary = await overview.json() as { campaigns: Array<{ key: string }>;
      canStart: boolean; csrfToken: string };
    assert.deepEqual(summary.campaigns.map(campaign => campaign.key), ['route-run']);
    assert.equal(summary.canStart, false);
    assert.equal(summary.csrfToken, 'route-token');
    assert.equal('plans' in summary, false);
    assert.deepEqual(await (await fetch(`${origin}/api/plans`)).json(), []);

    const sheet = await (await fetch(`${origin}/api/campaigns/route-run`)).json() as {
      key: string; stacks: Array<{ stack: string }> };
    assert.equal(sheet.key, 'route-run');
    assert.equal(sheet.stacks.length, 3);

    const checks = await (await fetch(
      `${origin}/api/campaigns/route-run/attempts/${attemptId}/checks`)).json() as {
        checks: Array<{ outcome: string }> };
    assert.ok(checks.checks.length > 0);
    const evidence = await (await fetch(
      `${origin}/api/campaigns/route-run/attempts/${attemptId}/package`)).json() as {
        executions: Array<{ artifacts: unknown[] }> };
    assert.equal(evidence.executions.length, 1);

    const log = await fetch(`${origin}/api/campaigns/route-run/attempts/${attemptId}/log?from=0`);
    assert.equal(log.headers.get('content-type'), 'text/plain; charset=utf-8');
    const offset = Number(log.headers.get('x-stack-bench-log-offset'));
    const text = await log.text();
    assert.match(text, /\[redacted credential\]/);
    assert.doesNotMatch(text, /secret-token-value/);
    assert.ok(offset > 0);
    const rest = await fetch(
      `${origin}/api/campaigns/route-run/attempts/${attemptId}/log?from=${offset}`);
    assert.equal(await rest.text(), '');
    assert.equal(Number(rest.headers.get('x-stack-bench-log-offset')), offset);

    assert.equal((await fetch(`${origin}/api/campaigns/route-run/progression`)).status, 404);
    assert.equal((await fetch(`${origin}/api/campaigns/missing-run`)).status, 404);
    assert.equal((await fetch(`${origin}/api/campaigns/Route-Run`)).status, 400);
    assert.equal((await fetch(
      `${origin}/api/campaigns/route-run/attempts/missing-attempt/checks`)).status, 404);
    assert.equal((await fetch(
      `${origin}/api/campaigns/route-run/attempts/${attemptId}/log?from=-1`)).status, 400);
    assert.equal((await fetch(
      `${origin}/api/campaigns/route-run/attempts/${attemptId}/log?from=eight`)).status, 400);
    const missing = await (await fetch(`${origin}/api/campaigns/missing-run`)).json() as {
      error: string };
    assert.equal(missing.error, 'Not found');

    for (const path of ['/', '/plans', '/c/route-run', `/c/route-run/a/${attemptId}`]) {
      const page = await fetch(`${origin}${path}`);
      assert.equal(page.status, 200, path);
      assert.equal(page.headers.get('cache-control'), 'no-store');
      assert.match(await page.text(), /<title>Stack Bench/);
    }
  });

async function nextServerEvents(origin: string, signal: AbortSignal, count: number,
  deadlineMs: number): Promise<string[]> {
  const response = await fetch(`${origin}/api/events`, { signal });
  assert.equal(response.headers.get('content-type'), 'text/event-stream; charset=utf-8');
  assert.ok(response.body);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const events: string[] = [];
  const deadline = setTimeout(() => { void reader.cancel(); }, deadlineMs);
  let buffered = '';
  try {
    while (events.length < count) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffered += decoder.decode(chunk.value, { stream: true });
      const frames = buffered.split('\n\n');
      buffered = frames.pop() ?? '';
      for (const frame of frames) {
        const match = frame.match(/^event: (\w+)\ndata: (.+)$/);
        if (match) events.push(`${match[1]} ${match[2]}`);
      }
    }
  } finally {
    clearTimeout(deadline);
  }
  return events;
}

test('the event stream reports a campaign write and the log bytes that follow it', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-events-'));
  const attemptId = writeSingleAttemptCampaign(root, 'event-run');
  const feed = { append() {}, list() { return []; } };
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: false, token: 'event-token', feed, plans: () => [] });
  const origin = await listenOrigin(server);
  const abort = new AbortController();
  t.after(() => { abort.abort(); server.close(); rmSync(root, { recursive: true, force: true }); });

  let recursive = true;
  try { watch(root, { recursive: true, persistent: false }).close(); }
  catch { recursive = false; }
  // Where the platform has no recursive watch the watcher polls every five
  // seconds instead; the same events arrive.
  const deadlineMs = recursive ? 1000 : 8000;

  const execution = join(root, 'campaigns', 'event-run', 'attempts', attemptId, 'execution-1');
  const events = nextServerEvents(origin, abort.signal, 2, deadlineMs);
  await new Promise(done => setTimeout(done, 50));
  const run = join(execution, 'run.json');
  writeFileSync(run, readFileSync(run, 'utf8'));
  appendFileSync(join(execution, 'process.stdout.log'), 'another line\n');
  assert.deepEqual(await events, ['campaign {"key":"event-run"}',
    `log {"key":"event-run","attemptId":"${attemptId}"}`]);
});

test('the progression view replays the graph once per stack within its budget', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-progression-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const plan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  assert.ok(plan.featureCatalog && plan.dependencyPolicy);
  const now = '2026-08-25T12:00:00.000Z';
  const directory = join(resultsRoot, 'campaigns', 'progression-run');
  let state = createCampaignState(plan, { now });
  const claims = [];
  for (;;) {
    const claimed = claimNextAttempt(state, { now, admissionId: 'admission-1' });
    if (!claimed.claim) break;
    claims.push(claimed.claim);
    state = claimed.state;
  }
  writeCampaign(directory, plan, state);
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  for (const claim of claims) {
    const output = join(directory, claim.output);
    mkdirSync(join(output, 'source'), { recursive: true });
    writeProgressionState(join(output, 'progression-state.json'), {
      progression,
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: { schemaVersion: 1,
        campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
        attempt: { id: claim.attempt.id, track: plan.definition.track,
          stack: claim.attempt.stack, agentAdapter: claim.attempt.agentAdapter,
          model: claim.attempt.model, conditionSha256: claim.attempt.condition.sha256 },
        workspace: { appDirectory: 'source' } },
      state: dependencyProgressionEvidence(plan, claim.attempt) });
  }

  const view = campaignProgression(resultsRoot, 'progression-run');
  assert.ok(view);
  assert.deepEqual(view.depths, plan.definition.levels);
  assert.ok(view.nodes.length > 0);
  assert.ok(view.nodes.every(node => view.depths.includes(node.depth)));
  assert.equal(view.stacks.length, claims.length);
  const track = view.stacks[0];
  assert.ok(track);
  assert.ok(track.steps.length > 0);
  assert.ok(track.steps.every(step => step.statuses.length === view.nodes.length));
  assert.ok(track.steps.some(step => step.action === 'build'));
  assert.equal(track.steps.some(step => step.action === 'repair'), false);
  assert.ok(track.steps.at(-1)?.statuses.includes('passed'));
  assert.equal(campaignProgression(resultsRoot, 'progression-run'), view);
  const bytes = Buffer.byteLength(JSON.stringify(view));
  assert.ok(bytes < 80 * 1024, `progression payload is ${bytes} bytes`);
});

// The client is a set of pure functions, so the rules the pages are held to —
// no prose, no badge, one value per cell, stacks in fixed order — are asserted
// here on rendered HTML rather than in a browser.

interface MarkupNode {
  tag: string;
  classes: string[];
}

interface MarkupText {
  text: string;
  path: MarkupNode[];
}

const VOID_TAGS = new Set(['img', 'br', 'line', 'circle', 'path', 'rect', 'meta', 'link', 'input']);
const CONTROL_TAGS = new Set(['a', 'button', 'select', 'summary']);
const LABEL_HOLDERS = new Set(['label', 'k', 'th']);
const DATA_HOLDERS = new Set(['v', 'big', 'phase', 'who', 'q', 'pct', 'd', 'h', 'shape', 'when',
  'stack', 'name', 'state', 'ev', 'band', 'log', 'links', 'files-list', 'title', 'crumbs', 'text',
  'group', 'b', 'live-head', 'facts', 'chart', 'sum', 'views', 'err', 'grade-key']);
const LABELS = new Set(['Campaign', 'Shape', 'Status', 'SpacetimeDB', 'PostgreSQL', 'MongoDB',
  'Stacks', 'Attempts', 'Parallel', 'State', 'Run name', 'Secret',
  'Updated', 'Mode', 'Depth', 'Levels', 'Work', 'Repair', 'Repairs', 'Repair budget', 'Repetitions',
  'Agent', 'Model', 'Guidance', 'Recipe', 'Recipes', 'Time limit', 'Spend limit', 'Controller',
  'Plan', 'Grading', 'Scope', 'Continued', 'Score', 'Unaided', 'Regressions', 'Time', 'Spend',
  'Climb', 'Attempt', 'Questline average', 'Evidence', 'Completed', 'Excluded', 'Check', 'Proves',
  'Grades', 'Now', 'Step', 'Stack', 'Action', 'Feature']);
const STATUS_WORDS = ['running', 'completed', 'ready', 'needs attention', 'unreadable', 'queued'];
const BANNED = [/\bfirst try\b/i, /\bfirst build\b/i, /\binitial\b/i, /\bbefore repairs\b/i,
  /\bverdict\b/i, /\bcontrol room\b/i];

function markupText(html: string): MarkupText[] {
  const found: MarkupText[] = [];
  const path: MarkupNode[] = [];
  const pattern = /<\/?([a-zA-Z][\w-]*)([^>]*)>/g;
  let index = 0;
  for (;;) {
    const tag = pattern.exec(html);
    const text = html.slice(index, tag?.index ?? html.length);
    if (text.trim()) found.push({ text: text.trim(), path: [...path] });
    if (!tag) break;
    index = pattern.lastIndex;
    const name = (tag[1] ?? '').toLowerCase();
    const attributes = tag[2] ?? '';
    if (tag[0].startsWith('</')) {
      const open = path.findLastIndex(entry => entry.tag === name);
      if (open >= 0) path.length = open;
      continue;
    }
    if (VOID_TAGS.has(name) || attributes.trimEnd().endsWith('/')) continue;
    path.push({ tag: name,
      classes: (/class="([^"]*)"/.exec(attributes)?.[1] ?? '').split(/\s+/).filter(Boolean) });
  }
  return found;
}

function dataShaped(text: string): boolean {
  return /^[-—·✓✕$/\d]/.test(text) || /^[a-z][a-z0-9.\-/ ]*$/.test(text);
}

function lintPage(name: string, html: string): void {
  assert.equal(/class="[^"]*\b(?:pill|badge)\b/.test(html), false, `${name} has a badge`);
  for (const banned of BANNED) assert.doesNotMatch(html, banned, `${name} uses a banned word`);
  for (const node of markupText(html)) {
    const control = node.path.some(entry => CONTROL_TAGS.has(entry.tag)
      || entry.classes.includes('chip') || entry.classes.includes('btn'));
    const label = node.path.some(entry => entry.classes.some(value => LABEL_HOLDERS.has(value))
      || LABEL_HOLDERS.has(entry.tag));
    const data = node.path.some(entry =>
      entry.classes.some(value => DATA_HOLDERS.has(value)) || DATA_HOLDERS.has(entry.tag));
    if (control) continue;
    if (label) {
      assert.ok(LABELS.has(node.text) || /^L\d+ (?:unaided|score)$/.test(node.text)
        || dataShaped(node.text) || node.path.some(entry => entry.classes.includes('q')),
      `${name} labels with "${node.text}"`);
      continue;
    }
    assert.ok(data || dataShaped(node.text),
      `${name} prints "${node.text}" that is neither data, a label nor a control`);
  }
}

function statusCells(html: string): string[] {
  return markupText(html)
    .filter(node => STATUS_WORDS.includes(node.text.toLowerCase())
      && !node.path.some(entry => entry.classes.some(value => LABEL_HOLDERS.has(value))
        || LABEL_HOLDERS.has(entry.tag)))
    .map(node => node.path.map(entry => entry.classes.join('.')).join('>'));
}

// Where the stacks are columns or rows they run in one order; the crumbs, the
// attempt title and the replay's selected event name a single stack.
function stackOrder(html: string): string[] {
  const single = new Set(['crumbs', 'title', 'ev', 'figs']);
  return markupText(html)
    .filter(node => ['SpacetimeDB', 'PostgreSQL', 'MongoDB'].includes(node.text)
      && !node.path.some(entry => entry.classes.some(value => single.has(value))))
    .map(node => node.text);
}

test('the client renders each page as data, labels and controls only', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-render-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  writeFixtureResults(resultsRoot);
  writeDependencyResults(resultsRoot, 'progression-run');

  const controllerActive = (): boolean => true;
  const overview = overviewSummary(join(resultsRoot, 'campaigns'), { controllerActive });
  const sequential = campaignSheet(resultsRoot, 'fixture-run-0', { controllerActive });
  const dependency = campaignSheet(resultsRoot, 'progression-run', { controllerActive });
  const progression = campaignProgression(resultsRoot, 'progression-run');
  assert.ok(progression);
  const attempt = sequential.stacks[0]?.attempts[0];
  assert.ok(attempt);
  const screenshotEvidence = attemptPackage(resultsRoot, 'fixture-run-0', attempt.id);
  screenshotEvidence.executions[0]?.visuals.push({ id: 'screenshot-id',
    path: 'grading/failure-media/example.png', name: 'Example failure', kind: 'visual',
    contentType: 'image/png', size: 8 });
  const plansRoot = join(resultsRoot, 'plans');
  writePlanFixtures(plansRoot);
  const plans = discoverPlans(plansRoot);
  const runForm = { planId: plans.find(plan => plan.state === 'frozen')?.id ?? '',
    outputName: 'ecommerce-20260902-1504', secret: '',
    error: 'That run output already exists.' };
  const pages: Array<[string, string]> = [
    ['campaigns', campaignsPage({ campaigns: overview, sheets: [dependency], filter: 'all' })],
    ['campaigns filtered',
      campaignsPage({ campaigns: overview, sheets: [], filter: 'completed' })],
    ['campaign sequential',
      campaignPage({ sheet: sequential, progression: null, view: 'grid', step: 0 })],
    ['campaign grid', campaignPage({ sheet: dependency, progression, view: 'grid', step: 0 })],
    ['campaign graph', campaignPage({ sheet: dependency, progression, view: 'graph', step: 0 })],
    ['campaign replay', campaignPage({ sheet: dependency, progression, view: 'replay', step: 3 })],
    ['plans', plansPage({ plans, canStart: false, form: runForm })],
    ['plans form', plansPage({ plans, canStart: true, form: runForm })],
    ['topbar plans', topbar({ page: 'plans', key: '', canStart: true, resumable: false, error: '' })],
    ['topbar resume', topbar({ page: 'campaign', key: 'progression-run', canStart: true,
      resumable: true, error: 'Only an interrupted campaign that is ready can resume.' })],
    ['attempt', attemptPage({ sheet: sequential, attemptId: attempt.id, tab: 'checks',
      checks: attemptChecks(resultsRoot, 'fixture-run-0', attempt.id),
      evidence: attemptPackage(resultsRoot, 'fixture-run-0', attempt.id), log: '' })],
    ['attempt screenshots', attemptPage({ sheet: sequential, attemptId: attempt.id,
      tab: 'screenshots', checks: null, evidence: screenshotEvidence, log: '' })],
  ];

  for (const [name, html] of pages) {
    lintPage(name, html);
    // Fixed stack order everywhere, and never a colour to tell them apart.
    const stacks = stackOrder(html);
    for (let index = 0; index + 2 < stacks.length; index += 3) {
      assert.deepEqual(stacks.slice(index, index + 3),
        ['SpacetimeDB', 'PostgreSQL', 'MongoDB'], `${name} reorders the stacks`);
    }
    // One value per sheet cell: a number, or a number over the total it is out of.
    for (const cell of html.match(/<div class="v">.*?<\/div>/g) ?? []) {
      const text = cell.replace(/<[^>]*>/g, ' ').trim();
      assert.match(text, /^\S+(?: \/ \S+)?$/, `${name} packs "${text}" into one cell`);
    }
    if (name.startsWith('campaigns')) continue;
    assert.deepEqual(statusCells(html), [], `${name} shows a status word outside the table`);
  }
  const campaigns = pages[0]![1];
  assert.ok(statusCells(campaigns).length > 0);
  const screenshots = pages.find(([name]) => name === 'attempt screenshots')?.[1] ?? '';
  assert.match(screenshots, /<button type="button" data-shot="[^"]+"/);
  assert.match(screenshots, /<dialog class="lightbox">/);
  for (const path of statusCells(campaigns)) {
    assert.match(path, /state/, 'a status word sits outside the Status column');
  }
  // Read-only host mode has the table and no form; only a frozen plan is offered.
  const readOnly = plansPage({ plans, canStart: false, form: runForm });
  const startable = plansPage({ plans, canStart: true, form: runForm });
  assert.equal(/<form/.test(readOnly), false);
  assert.deepEqual([...startable.matchAll(/<option value="([^"]*)"/g)].map(match => match[1]),
    plans.filter(plan => plan.state === 'frozen').map(plan => plan.id));
  assert.match(startable, /name="secret" type="password"/);
  // An unreadable plan is a row whose state is invalid and whose error is a hover.
  const invalid = /<span class="state \w+" title="([^"]+)">invalid</.exec(readOnly);
  assert.ok(invalid?.[1] && invalid[1] !== 'invalid');
  assert.equal(topbar({ page: 'campaigns', key: '', canStart: false, resumable: false, error: '' })
    .includes('Start a run'), false);
});
