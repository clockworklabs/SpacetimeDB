import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { request as httpRequest } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, writeArtifact } from '../artifacts.mjs';
import { campaignDetail, parseRunProgress, readCampaignArtifactBody, resolveCampaignArtifact,
  summarizeCampaign,
} from '../dashboard/dashboard-model.mjs';
import { createDashboardServer, parseDashboardArgs } from '../dashboard/dashboard-server.mjs';

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

test('dashboard can display an in-flight schema-1 campaign without reopening it for writes', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-v1-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const attemptPlan = { id: 'ecommerce-live-r1-postgres', stack: 'postgres', model: 'model',
    guidance: 'neutral', repetition: 1, levels: [1] };
  const plan = { campaignSchemaVersion: 1, id: 'ecommerce-live', version: '1.0.0', state: 'frozen',
    title: 'Live campaign', source: 'campaign.json', contentSha256: 'a'.repeat(64),
    definition: { track: 'ecommerce', levels: [1], repetitions: 1,
      budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 100 } },
    identities: {}, bindings: [], stacks: [{ id: 'postgres' }], agents: [], conditions: [],
    attempts: [attemptPlan], summary: { attempts: 1 } };
  const now = '2026-08-18T12:00:00.000Z';
  const state = { schemaVersion: 1, campaignId: plan.id, campaignSha256: plan.contentSha256,
    status: 'running', createdAt: now, updatedAt: now,
    attempts: [{ plan: attemptPlan, status: 'running', executions: [{
      id: `${attemptPlan.id}-execution1`, ordinal: 1, status: 'running',
      output: `attempts/${attemptPlan.id}/execution-1`, startedAt: now, completedAt: null,
      exitCode: null, outcome: null, reason: null, admissionId: 'admission-1',
    }] }], summary: { total: 1, completed: 0, invalid: 0, pending: 0, running: 1, executions: 1 } };
  writeArtifact(join(root, 'plan.json'), { kind: 'campaign_plan', id: `${plan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: plan });
  writeArtifact(join(root, 'state.json'), { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });
  const output = join(root, state.attempts[0].executions[0].output);
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'process.stdout.log'), '=== postgres-l1-first (postgres) ===\n  TOTAL ... 31/58\n--- fix round 1/3 ---\n');

  const summary = summarizeCampaign(root);
  assert.equal(summary.status, 'running');
  assert.equal(summary.maxParallel, 1);
  assert.equal(summary.attempts[0].progress.phase, 'Repairing L1 · round 1 of 3');
  assert.deepEqual(summary.attempts[0].progress.latestScore, { score: 31, max: 58 });
});

test('campaign detail exposes the evidence package but not arbitrary campaign files', async t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-package-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  const campaign = join(resultsRoot, 'campaigns', 'evidence-run');
  const attemptPlan = { id: 'evidence-postgres', stack: 'postgres', model: 'model',
    guidance: 'neutral', repetition: 1, levels: [1] };
  const plan = { campaignSchemaVersion: 1, id: 'evidence', version: '1.0.0', state: 'frozen',
    title: 'Evidence run', source: 'campaign.json', contentSha256: 'b'.repeat(64),
    definition: { track: 'ecommerce', levels: [1], repetitions: 1,
      budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 100 } },
    identities: {}, bindings: [], stacks: [{ id: 'postgres' }], agents: [], conditions: [],
    attempts: [attemptPlan], summary: { attempts: 1 } };
  const now = '2026-08-18T12:00:00.000Z';
  const outputRelative = `attempts/${attemptPlan.id}/execution-1`;
  const state = { schemaVersion: 1, campaignId: plan.id, campaignSha256: plan.contentSha256,
    status: 'completed', createdAt: now, updatedAt: now,
    attempts: [{ plan: attemptPlan, status: 'completed', executions: [{
      id: `${attemptPlan.id}-execution1`, ordinal: 1, status: 'completed',
      output: outputRelative, startedAt: now, completedAt: now, exitCode: 0,
      outcome: 'completed', reason: null, admissionId: 'admission-1',
    }] }], summary: { total: 1, completed: 1, invalid: 0, pending: 0, running: 0, executions: 1 } };
  mkdirSync(campaign, { recursive: true });
  writeArtifact(join(campaign, 'plan.json'), { kind: 'campaign_plan', id: `${plan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: plan });
  writeArtifact(join(campaign, 'state.json'), { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });
  const output = join(campaign, outputRelative);
  const media = join(output, 'grading', 'failure-media');
  mkdirSync(media, { recursive: true });
  writeFileSync(join(output, 'process.stdout.log'), 'authorization: Bearer secret-token-value\nresult ok\n');
  writeFileSync(join(output, 'run.json'), '{"result":"ok"}\n');
  writeFileSync(join(media, 'failed-check.png'), Buffer.from('89504e470d0a1a0a', 'hex'));
  mkdirSync(join(campaign, 'admissions'), { recursive: true });
  writeFileSync(join(campaign, 'admissions', 'private.json'), '{"token":"do-not-serve"}\n');

  const detail = campaignDetail(resultsRoot, 'evidence-run');
  assert.deepEqual(detail.package.campaign.map(item => item.path), ['plan.json', 'state.json']);
  assert.equal(detail.package.executions.length, 1);
  assert.equal(detail.package.executions[0].truncated, false);
  assert.equal(detail.package.executions[0].visuals[0].path,
    `${outputRelative}/grading/failure-media/failed-check.png`);
  const log = detail.package.executions[0].artifacts.find(item => item.path.endsWith('process.stdout.log'));
  assert.match(readCampaignArtifactBody(resolveCampaignArtifact(resultsRoot, 'evidence-run', log.id)).toString(),
    /\[redacted credential\]/);
  assert.throws(() => resolveCampaignArtifact(resultsRoot, 'evidence-run',
    Buffer.from('admissions/private.json').toString('base64url')), /not available/);
  assert.throws(() => resolveCampaignArtifact(resultsRoot, 'evidence-run',
    Buffer.from('../outside.json').toString('base64url')), /not available|outside/);

  const feed = { append() {}, list() { return []; } };
  const { server } = createDashboardServer({ resultsRoot, plansRoot: join(resultsRoot, 'plans'),
    allowLaunch: false, token: 'package-token', feed, plans: () => [] });
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  t.after(() => server.close());
  const origin = `http://127.0.0.1:${server.address().port}`;
  const visual = detail.package.executions[0].visuals[0];
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

test('dashboard serves real state and protects campaign launch with its local session token', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-'));
  const plansRoot = join(root, 'plans');
  mkdirSync(plansRoot, { recursive: true });
  const events = [];
  const launches = [];
  const feed = {
    append(event) { events.push(structuredClone(event)); },
    list() {
      const latest = new Map();
      for (const event of events) latest.set(event.id, { ...(latest.get(event.id) ?? {}), ...event });
      return [...latest.values()];
    },
  };
  const frozenPlan = { id: 'ecommerce-frozen', version: '1.0.0', title: 'Frozen ecommerce',
    state: 'frozen', track: 'ecommerce', levels: [1, 2], stacks: ['postgres', 'mongodb'],
    attempts: 2, parallelism: 2, budgets: { fixRounds: 3, attemptTimeoutMinutes: 240 },
    sha256: 'a'.repeat(64), file: 'ecommerce-frozen.json' };
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot, allowLaunch: true,
    token: 'test-session-token', feed, plans: () => [frozenPlan],
    launch(input) { launches.push(input); return { pid: 1234 }; } });
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });
  const origin = `http://127.0.0.1:${server.address().port}`;

  const page = await fetch(origin);
  assert.equal(page.status, 200);
  const pageHtml = await page.text();
  assert.match(pageHtml, /StackBench Control Room/);
  assert.match(pageHtml, /id="history-prev"/);
  assert.doesNotMatch(pageHtml, /Controller activity/);
  const brand = await fetch(`${origin}/spacetimedb-mark.svg`);
  assert.equal(brand.status, 200);
  assert.equal(brand.headers.get('content-type'), 'image/svg+xml');
  assert.match(await brand.text(), /viewBox="0 0 35 32"/);
  const overview = await (await fetch(`${origin}/api/overview`)).json();
  assert.equal(overview.canStart, true);
  assert.equal(overview.csrfToken, 'test-session-token');
  assert.deepEqual(overview.plans.map(plan => plan.id), ['ecommerce-frozen']);

  const reboundStatus = await new Promise((resolveStatus, reject) => {
    const rebound = httpRequest(`${origin}/api/overview`, { headers: { host: 'attacker.invalid' } },
      response => { response.resume(); response.once('end', () => resolveStatus(response.statusCode)); });
    rebound.once('error', reject);
    rebound.end();
  });
  assert.equal(reboundStatus, 421);

  const rejected = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(rejected.status, 403);
  assert.equal(launches.length, 0);

  const accepted = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-session-token' },
    body: JSON.stringify({ planId: frozenPlan.id, outputName: 'meeting-run-1' }) });
  assert.equal(accepted.status, 202);
  assert.equal(launches.length, 1);
  assert.equal(launches[0].plan.id, frozenPlan.id);
  assert.equal(launches[0].output, join(root, 'campaigns', 'meeting-run-1'));
  assert.equal(feed.list()[0].status, 'running');
  assert.equal(feed.list()[0].pid, 1234);
  assert.equal(feed.list()[0].campaignSha256, frozenPlan.sha256);
});

test('host development mode is read-only even with a valid browser request', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dashboard-readonly-'));
  mkdirSync(join(root, 'plans'), { recursive: true });
  const feed = { append() {}, list() { return []; } };
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: false, token: 'readonly-token', feed, plans: () => [] });
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  t.after(() => { server.close(); rmSync(root, { recursive: true, force: true }); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const response = await fetch(`${origin}/api/campaigns`, { method: 'POST',
    headers: { origin, 'content-type': 'application/json', 'x-stack-bench-token': 'readonly-token' },
    body: JSON.stringify({ planId: 'anything', outputName: 'meeting-run-2' }) });
  assert.equal(response.status, 503);
});
