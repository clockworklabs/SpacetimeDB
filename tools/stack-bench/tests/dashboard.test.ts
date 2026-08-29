import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { request as httpRequest } from 'node:http';
import type { Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { EventEmitter } from 'node:events';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution }
  from '../src/campaigns/campaign-scheduler.js';
import type { CampaignState } from '../src/campaigns/campaign-scheduler.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.mjs';
import { campaignDetail, campaignFacts, firstGradeAbort, parseRunProgress,
  readCampaignArtifactBody, readJsonLines, resolveCampaignArtifact, summarizeCampaign,
} from '../dashboard/dashboard-model.js';
import { createDashboardServer, parseDashboardArgs } from '../dashboard/dashboard-server.js';
import type { DashboardOperation, LaunchInput } from '../dashboard/dashboard-server.js';
import { sha256 } from '../src/evidence/provenance.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { writeProgressionState } from '../src/progression/progression-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const EXAMPLE_CAMPAIGN = join(STACK_BENCH_ROOT, 'appliance', 'campaign.example.json');
const DEPENDENCY_CAMPAIGN = join(STACK_BENCH_ROOT, 'appliance',
  'campaign.ecommerce-progression-reference.json');

function writeCampaign(root: string, plan: CompiledCampaignPlan, state: CampaignState): void {
  mkdirSync(root, { recursive: true });
  writeArtifact(join(root, 'plan.json'), { kind: 'campaign_plan', id: `${plan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: plan });
  writeArtifact(join(root, 'state.json'), { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });
}

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
--- fix round 1/3 ---
=== postgres-l1-fix1 (postgres) ===
  TOTAL      ... 41/58
--- fix round 2/3 ---
=== postgres-l1-fix2 (postgres) ===
`, { fixRounds: 3 });
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
--- fix round 1/10 ---
=== spacetime-l1-fix1 (spacetime) ===
  TOTAL      ... 58/58
=== spacetime-l2 (spacetime) ===
  TOTAL      ... 52/59
--- fix round 1/10 ---
`, { fixRounds: 10 });
  assert.equal(progress.level, 2);
  assert.equal(progress.phase, 'Repairing L2 · round 1 of 10');
  assert.deepEqual(progress.firstScore, { score: 52, max: 58 });
  assert.deepEqual(progress.latestScore, { score: 52, max: 59 });
});

test('an aborted first grade is classified, not treated as a scored zero', () => {
  // The postgres r3 shape from the 20260823 cohort: the seed probe stopped the
  // grade before any suite ran, so 0/58 must read as an abort with its reason.
  assert.deepEqual(firstGradeAbort({ score: 0, max: 58, outcome: { kind: 'app_failure',
    phase: 'application-seed', reason: 'startup data is missing: items contains 0 entries' } }),
  { phase: 'application-seed', reason: 'startup data is missing: items contains 0 entries' });
  // A graded first build — even a zero — is a score, not an abort.
  assert.equal(firstGradeAbort({ score: 0, max: 58,
    outcome: { kind: 'app_failure', phase: 'grading', reason: null } }), null);
  assert.equal(firstGradeAbort({ score: 31, max: 58,
    outcome: { kind: 'passed', phase: 'grading' } }), null);
  assert.equal(firstGradeAbort(null), null);
  assert.equal(firstGradeAbort({ score: 31, max: 58 }), null);
});

test('campaign facts surface the identity an operator otherwise reads plan.json for', () => {
  const facts = campaignFacts({
    agents: [{ adapter: 'claude-code', adapterVersion: '1.12.0', model: 'claude-sonnet-5',
      costLimit: 'native', identity: { id: 'claude-code', version: '1.12.0',
        sha256: 'b'.repeat(64) } }],
    attempts: [{ condition: { requested: { levels: [
      { level: 1, recipe: { id: 'ecommerce.l1-modular', version: '2.4.0' } },
      { level: 2, recipe: { id: 'ecommerce.l2-standard', version: '1.5.0' } },
    ] } } }],
    definition: { runtime: { controllerImage: 'stack-bench-controller@sha256:' + 'a'.repeat(64),
      buildImage: null } },
  });
  assert.deepEqual(facts.agents, [{ adapter: 'claude-code', version: '1.12.0', model: 'claude-sonnet-5' }]);
  assert.deepEqual(facts.recipes, [
    { level: 1, id: 'ecommerce.l1-modular', version: '2.4.0' },
    { level: 2, id: 'ecommerce.l2-standard', version: '1.5.0' },
  ]);
  assert.ok(facts.runtime);
  assert.equal(facts.runtime.controllerImage, 'stack-bench-controller@sha256:' + 'a'.repeat(64));
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
  assert.equal(firstAttempt.dependency.level, 1);
  assert(firstAttempt.dependency.work.current.some(node => node.id === 'accounts'));
  assert.deepEqual(firstAttempt.dependency.attempts,
    { total: 0, level: 0, maxRemaining: 1, features: [
      { nodeId: 'accounts', initialBudget: 1, granted: 0, budget: 1, used: 0, remaining: 1 },
      { nodeId: 'catalog', initialBudget: 1, granted: 0, budget: 1, used: 0, remaining: 1 },
      { nodeId: 'staff-access', initialBudget: 1, granted: 0, budget: 1, used: 0, remaining: 1 },
      { nodeId: 'support-intake', initialBudget: 1, granted: 0, budget: 1, used: 0, remaining: 1 },
    ] });
  assert.ok(summary.package);
  assert(summary.package.executions[0]?.artifacts
    .some(item => item.path.endsWith('/progression-state.json')));
});

test('a prepared attempt is waiting rather than finished', () => {
  const progress = parseRunProgress('', { fixRounds: 10, running: false, status: 'pending' });
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
    '=== postgres-l1-first (postgres) ===\n  TOTAL ... 31/58\n--- fix round 1/3 ---\n');

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

test('dashboard keeps a current-schema campaign readable after the controller is upgraded', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-historical-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const currentPlan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const state = createCampaignState(currentPlan, { now: '2026-08-18T12:00:00.000Z' });
  const historicalPlan = structuredClone(currentPlan);
  historicalPlan.identities.engine.sha256 = 'f'.repeat(64);
  historicalPlan.contentSha256 = sha256(canonicalDefinitionJson({
    campaignSchemaVersion: historicalPlan.campaignSchemaVersion,
    definition: historicalPlan.definition,
    engine: historicalPlan.identities.engine,
    bindings: historicalPlan.bindings,
    stacks: historicalPlan.stacks,
    agents: historicalPlan.agents,
    conditions: historicalPlan.conditions,
  }));
  state.campaignSha256 = historicalPlan.contentSha256;
  writeArtifact(join(root, 'plan.json'), { kind: 'campaign_plan', id: `${historicalPlan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: historicalPlan });
  writeArtifact(join(root, 'state.json'), { kind: 'campaign_state', id: `${historicalPlan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });

  const summary = summarizeCampaign(root);
  assert.equal(summary.id, historicalPlan.id);
  assert.equal(summary.status, 'prepared');
  assert.equal(summary.attempts.length, historicalPlan.attempts.length);
});

test('campaign detail exposes the evidence package but not arbitrary campaign files', async t => {
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
  writeFileSync(join(media, 'failed-check.png'), Buffer.from('89504e470d0a1a0a', 'hex'));
  mkdirSync(join(campaign, 'admissions'), { recursive: true });
  writeFileSync(join(campaign, 'admissions', 'private.json'), '{"token":"do-not-serve"}\n');

  const detail = campaignDetail(resultsRoot, 'evidence-run');
  assert.ok(detail.package);
  const packageExecution = detail.package.executions[0];
  assert.ok(packageExecution);
  assert.deepEqual(detail.package.campaign.map(item => item.path), ['plan.json', 'state.json']);
  assert.equal(detail.package.executions.length, 1);
  assert.equal(packageExecution.truncated, false);
  const initialVisual = packageExecution.visuals[0];
  assert.ok(initialVisual);
  assert.equal(initialVisual.path,
    `${outputRelative}/grading/failure-media/failed-check.png`);
  const log = packageExecution.artifacts.find(item => item.path.endsWith('process.stdout.log'));
  assert.ok(log);
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
  assert.match(pageHtml, /Stack Bench Control Room/);
  assert.match(pageHtml, /id="campaign-list"/);
  assert.match(pageHtml, /class="primary topbar-run"/);
  assert.doesNotMatch(pageHtml, /Controller activity/);
  assert.doesNotMatch(pageHtml, /See every run/);
  const brand = await fetch(`${origin}/spacetimedb-mark.svg`);
  assert.equal(brand.status, 200);
  assert.equal(brand.headers.get('content-type'), 'image/svg+xml');
  assert.match(await brand.text(), /viewBox="0 0 35 32"/);
  const overview = await (await fetch(`${origin}/api/overview`)).json() as {
    canStart: boolean;
    csrfToken: string;
    plans: Array<{ id: string }>;
  };
  assert.equal(overview.canStart, true);
  assert.equal(overview.csrfToken, 'test-session-token');
  assert.doesNotMatch(JSON.stringify(overview), /test-control-secret-value/);
  assert.deepEqual(overview.plans.map(plan => plan.id), [frozenPlan.id]);

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
