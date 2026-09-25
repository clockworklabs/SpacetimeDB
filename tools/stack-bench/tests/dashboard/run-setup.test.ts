import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import test from 'node:test';
import { prepareRun, runSetupCatalog, submitPreparedRun } from '../../src/campaigns/run-setup.js';
import { createDashboardServer, type LaunchInput } from '../../dashboard/dashboard-server.js';
import { initialRun, runSetupPage, selectGuidance } from '../../dashboard/public/views/run-setup.js';
import { writePlanFixtures } from '../fixtures/dashboard-fixture.js';
import { STACK_BENCH_ROOT } from '../../src/package-root.js';

// One invariant: only the exact reviewed configuration can dispatch, and retries
// dispatch it once. No fixture invokes a controller, database, or model.
test('setup reviews exact dimensions, rejects changes, and dispatches one durable job', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-setup-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writePlanFixtures(join(root, 'plans'));
  mkdirSync(join(root, 'run-presets'));
  const presetPath = join(root, 'run-presets', 'ecommerce.json');
  const source = readFileSync(join(root, 'plans', 'ecommerce-progression-reference.json'), 'utf8');
  writeFileSync(presetPath, source);
  const catalog = runSetupCatalog(root, {});
  const choices = structuredClone(catalog);
  choices.workloads[0]!.conditions = [
    { id: 'neutral-dev-no-sdk', guidance: 'neutral-dev-no-sdk', sdkSkills: false, devWorkflow: true },
    { id: 'neutral', guidance: 'neutral', sdkSkills: true, devWorkflow: false },
    { id: 'neutral-no-sdk', guidance: 'neutral-no-sdk', sdkSkills: false, devWorkflow: false },
    { id: 'neutral-dev', guidance: 'neutral-dev', sdkSkills: true, devWorkflow: true },
  ];
  const initial = initialRun(choices)!;
  assert.equal(initial.productionQuality, true);
  assert.deepEqual(initial.conditions, ['neutral-dev']);
  const single = { ...choices.workloads[0]!, id: 'single-build', workSelection: 'all-at-once' as const };
  const reordered = { ...choices, workloads: [single, ...choices.workloads] };
  assert.equal(initialRun(reordered)!.workload, choices.workloads[0]!.id);
  assert.equal(initialRun(reordered, single.id)!.workload, single.id);
  const conditions = choices.workloads[0]!.conditions;
  for (const c of conditions) assert.deepEqual(selectGuidance(conditions, c.sdkSkills ? 'on' : 'off',
    c.devWorkflow ? 'on' : 'off'), [c.id]);
  const request = { ...initialRun(catalog)!, key: 'setup-smoke', level: 1,
    repetitions: 2, parallelism: 6, maxCostUsd: 12, repairs: 0, pauseAfterDepth: null };
  const review = prepareRun(root, request, {});
  assert.equal(review.attempts, 6);
  assert.equal(review.parallelism, 6);
  assert.equal(review.maxCostUsd, 72);
  assert.equal(review.mode.workSelection, 'progressive');
  assert.equal(review.request.productionQuality, true);
  const prototypeReview = prepareRun(root, { ...request, productionQuality: false }, {});
  assert.notEqual(prototypeReview.planSha256, review.planSha256);
  assert.match(runSetupPage(catalog, request, review, '', true), /Production-quality app<\/dt><dd>Requested/);
  assert.match(runSetupPage(catalog, prototypeReview.request, prototypeReview, '', true),
    /Production-quality app<\/dt><dd>Not requested/);
  assert.equal(prepareRun(root, request, {}).reviewId, review.reviewId);
  assert.equal(existsSync(join(root, 'jobs')), false);
  assert.throws(() => prepareRun(root, { ...request, stacks: [...request.stacks, request.stacks[0]] }, {}), /duplicates/);
  assert.throws(() => prepareRun(root, { ...request, level: 99 }, {}), /Target level/);
  assert.throws(() => prepareRun(root, { ...request, parallelism: 0 }, {}));
  assert.throws(() => prepareRun(root, { ...request, workload: '../escape' }, {}));
  assert.throws(() => submitPreparedRun(root, { request: { ...request, maxCostUsd: 24 }, reviewId: review.reviewId }, {}), /Setup changed/);
  assert.throws(() => submitPreparedRun(root,
    { request: { ...request, productionQuality: false }, reviewId: review.reviewId }, {}), /Setup changed/);
  const changed = JSON.parse(source); changed.pricing.models['reference-fixture'].input = 1;
  writeFileSync(presetPath, JSON.stringify(changed));
  assert.throws(() => submitPreparedRun(root, review, {}), /Workload changed/);
  writeFileSync(presetPath, source);
  assert.throws(() => prepareRun(root, request, { STACK_BENCH_CONTROLLER_IMAGE: 'different' }), /runtime/);

  assert.doesNotMatch(runSetupPage(catalog, request, review, '', true), /Operator secret|name="secret"/);
  assert.doesNotMatch(runSetupPage(catalog, request, null, '', true), /name="secret"/);
  const launches: LaunchInput[] = [];
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: true, token: 'test-token',
    launch(input) { launches.push(input); return Object.assign(new EventEmitter(), { pid: 1 }); } });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  const send = (path: string, value: unknown, token = 'test-token') =>
    fetch(origin + path, { method: 'POST', headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': token }, body: JSON.stringify(value) });
  assert.equal((await send('/api/runs', review, 'wrong')).status, 403);
  const preparation = await send('/api/runs/prepare', request);
  assert.equal(preparation.status, 200, await preparation.clone().text());
  const acceptedReview = await preparation.json();
  assert.equal(launches.length, 0);
  const first = await send('/api/runs', acceptedReview);
  assert.equal(first.status, 202, await first.clone().text());
  const result = await first.json() as { campaignKey: string; job: { id: string } };
  assert.equal((await send('/api/runs', acceptedReview)).status, 202);
  assert.equal((await send(`/api/jobs/${result.job.id}/start`, {})).status, 202);
  assert.equal(launches.length, 1);
  assert.equal(launches[0]!.command, 'work');
  assert.equal(launches[0]!.jobId, result.job.id);
  const pending = await (await fetch(`${origin}/api/campaigns/${result.campaignKey}`)).json() as { pendingJob: { status: string } };
  assert.equal(pending.pendingJob.status, 'queued');
  assert.equal((await send(`/api/jobs/${result.job.id}/cancel`, {})).status, 202);
  assert.equal((await send('/api/runs', acceptedReview)).status, 202);
  assert.equal(launches.length, 1, 'cancelled work must not restart');
});

test('the four-stack L1–L3 preset exposes Convex and prepares only supported levels', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-convex-setup-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writePlanFixtures(join(root, 'plans'));
  mkdirSync(join(root, 'run-presets'));
  const preset = JSON.parse(readFileSync(join(root, 'plans', 'ecommerce-progression-reference.json'), 'utf8'));
  preset.id = 'ecommerce-progressive-l3';
  preset.levels = [1, 2, 3];
  preset.selection.levels = preset.selection.levels.filter((selection: { level: number }) => selection.level <= 3);
  preset.stacks.push({ id: 'convex', adapterVersion: '1.0.0' });
  writeFileSync(join(root, 'run-presets', `${preset.id}.json`), JSON.stringify(preset));
  const catalog = runSetupCatalog(root, {});
  const request = { ...initialRun(catalog)!, key: 'convex-setup', stacks: ['convex'], maxCostUsd: 12 };
  assert.equal(request.level, 3);
  assert.match(runSetupPage(catalog, request, null, '', true), /value="convex" checked>Convex/);
  const review = prepareRun(root, request, {});
  assert.deepEqual(review.request.stacks, ['convex']);
  assert.equal(review.attempts, preset.repetitions);
  const saved = JSON.parse(readFileSync(review.planFile, 'utf8'));
  assert.deepEqual(saved.stacks.map((stack: { id: string }) => stack.id), ['convex']);
  assert.deepEqual(saved.levels, [1, 2, 3]);
  assert.throws(() => prepareRun(root, { ...request, level: 4 }, {}), /Target level is not in this workload/);
  assert.equal(existsSync(join(root, 'jobs')), false, 'preparation must not launch work');
});

test('automatic accounts are explicit in the review and ambiguous accounts require selection', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-accounts-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writePlanFixtures(join(root, 'plans'));
  mkdirSync(join(root, 'run-presets'));
  const preset = JSON.parse(readFileSync(join(root, 'plans', 'ecommerce-progression-reference.json'), 'utf8'));
  preset.agents = [{ adapter: 'codex', adapterVersion: '1.0.0', model: 'gpt-6-astra', effort: 'medium' },
    { adapter: 'claude-code', adapterVersion: '1.17.2', model: 'claude-sonnet-5', effort: 'medium' }];
  for (const a of preset.agents) preset.pricing.models[a.model] = preset.pricing.models['reference-fixture'];
  writeFileSync(join(root, 'run-presets', 'accounts.json'), JSON.stringify(preset));
  const secretFile = join(root, 'synthetic-secret');
  writeFileSync(secretFile, 'SYNTHETIC_ONLY');
  const profiles = { openai: { provider: 'openai', mode: 'api-key', version: '1', secretFile },
    claude: { provider: 'anthropic', mode: 'api-key', version: '1', secretFile } };
  const registry = join(root, 'profiles.json');
  writeFileSync(registry, JSON.stringify(profiles));
  const env = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry };
  const catalog = runSetupCatalog(root, env);
  const request = { ...initialRun(catalog)!, level: 1, maxCostUsd: 12,
    agents: catalog.workloads[0]!.agents.map((_, index) => ({ index, effort: 'medium' as const })) };
  const review = prepareRun(root, request, env);
  assert.deepEqual(review.request.credentials.adapters, { 'claude-code': 'claude', codex: 'openai' });
  assert.equal(prepareRun(root, review.request, env).reviewId, review.reviewId);
  assert.deepEqual(request.credentials, {});
  assert.doesNotMatch(JSON.stringify(review), /SYNTHETIC_ONLY/);
  writeFileSync(registry, JSON.stringify({ ...profiles, other: profiles.openai }));
  const saved = readdirSync(join(root, 'plans'));
  assert.throws(() => prepareRun(root, request, env), /Choose an account for codex/);
  assert.throws(() => prepareRun(root, { ...request, key: 'rejected-review' }, env), /Choose an account for codex/);
  assert.deepEqual(readdirSync(join(root, 'plans')), saved, 'a rejected review must not save a plan');
  assert.equal(prepareRun(root, review.request, env).reviewId, review.reviewId);
  assert.equal(existsSync(join(root, 'jobs')), false);
});

test('a new provider model freezes its declared rates and output bound before dispatch', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-new-model-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'run-presets'));
  const preset = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'), 'utf8'));
  preset.state = 'frozen';
  preset.budgets.maxCostUsdPerAttempt = 12;
  preset.runtime.controllerImage = `sha256:${'a'.repeat(64)}`;
  preset.runtime.buildImage = `sha256:${'b'.repeat(64)}`;
  preset.agents = [{ adapter: 'codex', adapterVersion: '1.0.0', model: 'gpt-6-astra', effort: 'medium' },
    { adapter: 'claude-code', adapterVersion: '1.17.2', model: 'claude-sonnet-5', effort: 'medium' },
    { adapter: 'openrouter', adapterVersion: '1.0.0', model: 'openai/gpt-5.3-codex',
      effort: 'medium', providerRoute: 'openai', maxOutputTokens: 8192 }];
  preset.pricing.models['gpt-6-astra'] = preset.pricing.models.deterministic;
  preset.pricing.models['claude-sonnet-5'] = preset.pricing.models.deterministic;
  preset.pricing.models['openai/gpt-5.3-codex'] = preset.pricing.models.deterministic;
  writeFileSync(join(root, 'run-presets', 'models.json'), JSON.stringify(preset));
  const secretFile = join(root, 'synthetic-secret');
  writeFileSync(secretFile, 'SYNTHETIC_ONLY');
  const registry = join(root, 'profiles.json');
  writeFileSync(registry, JSON.stringify({ openai: { provider: 'openai', mode: 'api-key', version: '1', secretFile },
    anthropic: { provider: 'anthropic', mode: 'api-key', version: '1', secretFile },
    openrouter: { provider: 'openrouter', mode: 'api-key', version: '1', secretFile } }));
  const env = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry };
  const catalog = runSetupCatalog(root, env);
  const modelIndex = (adapter: string) => catalog.workloads[0]!.agents.findIndex(a => a.adapter === adapter);
  const request = { ...initialRun(catalog)!, key: 'new-model', level: 1,
    stacks: ['postgres'], maxCostUsd: 12, agents: [{ index: modelIndex('codex'), effort: 'medium' as const,
      model: 'gpt-6-sol', maxOutputTokens: 128_000,
      outputLimitSource: 'https://example.test/model-limit',
      pricing: { input: 1, output: 5, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
      pricingSource: 'https://example.test/pricing', pricingCapturedAt: '2026-09-25T00:00:00.000Z' }] };
  const review = prepareRun(root, request, env);
  const saved = JSON.parse(readFileSync(review.planFile, 'utf8'));
  assert.equal(saved.agents[0].model, 'gpt-6-sol');
  assert.equal(saved.agents[0].maxOutputTokens, 128_000);
  assert.equal(saved.pricing.models['gpt-6-sol'].output, 5);
  assert.match(saved.pricing.source, /https:\/\/example.test\/pricing/);
  for (const [adapter, model, extra] of [
    ['claude-code', 'claude-opus-5-5', {}],
    ['openrouter', 'openai/gpt-5.4', { providerRoute: 'openai', maxOutputTokens: 8192,
      outputLimitSource: 'https://example.test/router-limit' }],
  ] as const) {
    const other = prepareRun(root, { ...request, key: `new-model-${adapter}`, agents: [{ index: modelIndex(adapter), effort: 'medium',
      model, pricing: { input: 1, output: 5, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
      pricingSource: 'https://example.test/pricing', pricingCapturedAt: '2026-09-25T00:00:00.000Z',
      ...extra }] }, env);
    const definition = JSON.parse(readFileSync(other.planFile, 'utf8'));
    assert.equal(definition.agents[0].model, model);
  }
  assert.throws(() => prepareRun(root, { ...request, key: 'missing-price',
    agents: [{ index: 0, effort: 'medium', model: 'gpt-6-sol' }] }, env), /pricing/i);
  assert.throws(() => submitPreparedRun(root, { request: { ...request,
    agents: [{ ...request.agents[0]!, pricing: { ...request.agents[0]!.pricing, output: 6 } }] },
    reviewId: review.reviewId }, env), /Setup changed/);
  assert.equal(existsSync(join(root, 'jobs')), false);
});
