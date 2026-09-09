import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runCampaignAdmission, validateCampaignAdmission }
  from '../src/campaigns/campaign-admission.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAdmissionPreflightRequest }
  from '../src/campaigns/campaign-admission.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const createdAt = '2026-08-27T12:00:00.000Z';
const passingPreflight = (request: CampaignAdmissionPreflightRequest) => ({
  schemaVersion: 1 as const,
  generatedAt: createdAt,
  request: { backends: request.backends, track: request.track, levels: request.levelList,
    runIndex: request.runIndex, parallelism: request.parallelism,
    agentAdapter: request.agentAdapter,
    ...(request.providerRoute ? { providerRoute: request.providerRoute } : {}),
    ...(request.maxOutputTokens ? { maxOutputTokens: request.maxOutputTokens } : {}),
    packs: request.packIds, checks: request.checkKeys, image: request.image,
    resultsDir: request.resultsDir, smoke: request.smoke },
  ok: true,
  summary: { passed: 1, failed: 0, warnings: 0 },
  checks: [{ id: 'smoke.container', status: 'pass' as const, summary: 'passed' }],
});

test('campaign admission receives only the feature catalog levels in the compiled plan', { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-scoped-admission-'));
  try {
    const value = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'appliance',
      'campaign.ecommerce-progression-reference.json'), 'utf8'));
    value.levels = [1, 2, 3];
    value.selection.levels = value.selection.levels.filter((entry: { level: number }) =>
      entry.level <= 3);
    const campaignPath = join(root, 'campaign.json');
    writeFileSync(campaignPath, `${JSON.stringify(value, null, 2)}\n`);
    const plan = compileCampaignFile(campaignPath);
    const requests: CampaignAdmissionPreflightRequest[] = [];
    const result = await runCampaignAdmission(plan, root, {       now: createdAt,
      uuid: () => 'scoped',
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks') },
      preflight: request => {
        requests.push(request);
        return passingPreflight(request);
      },
    });

    assert.equal(result.payload.ok, true);
    assert.equal(requests.length, plan.summary.parallelism);
    assert(requests.every(request => request.featureCatalog!.definition.nodes
      .every(node => node.level <= 3)));
    assert(plan.featureCatalog);
    const identity = plan.featureCatalog.identity;
    assert(requests.every(request => request.featureCatalog!.identity.contentSha256
      === identity.contentSha256));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('campaign admission selects a free run slot', { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-free-slot-admission-'));
  try {
    const value = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'tests', 'fixtures',
      'campaign.deterministic.json'), 'utf8'));
    value.repetitions = 1;
    const campaignPath = join(root, 'campaign.json');
    writeFileSync(campaignPath, `${JSON.stringify(value, null, 2)}\n`);
    const plan = compileCampaignFile(campaignPath);
    const requests: CampaignAdmissionPreflightRequest[] = [];
    let portProbes = 0;
    const result = await runCampaignAdmission(plan, root, {       now: createdAt,
      uuid: () => 'free-slot',
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks') },
      probePort: () => ({ free: ++portProbes > 1 }),
      preflight: request => {
        requests.push(request);
        return passingPreflight(request);
      },
    });
    assert.deepEqual(result.runIndices, [1]);
    assert.equal(requests.length, 1);
    assert.equal(requests[0]!.runIndex, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('admission requires a distinct report for each provider route and rejects substituted routes', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-route-admission-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures',
      'campaign.deterministic.json'));
    const agent = plan.agents[0]!;
    plan.agents = ['openai', 'azure'].map(providerRoute => ({ ...agent,
      adapter: 'openrouter', providerRoute, maxOutputTokens: 8192 }));
    const payload = { schemaVersion: 1, campaignId: plan.id, campaignSha256: plan.contentSha256,
      createdAt, ok: true, runtime: plan.definition.runtime, conditions: plan.conditions,
      agents: plan.agents.map(({ adapter, model, providerRoute, maxOutputTokens, identity }) =>
        ({ adapter, model, providerRoute, maxOutputTokens, identity })),
      reports: plan.agents.flatMap(({ adapter, providerRoute, maxOutputTokens }) =>
        Array.from({ length: plan.summary.parallelism }, (_, runIndex) => ({
          schemaVersion: 1, ok: true, checks: [], summary: { passed: 0, failed: 0, warnings: 0 },
          request: { agentAdapter: adapter, providerRoute, maxOutputTokens, runIndex,
            backends: plan.stacks.map(stack => stack.id), track: plan.definition.track,
            levels: plan.definition.levels, parallelism: plan.summary.parallelism,
            packs: plan.definition.selection.packs ?? [], checks: plan.definition.selection.checks ?? [],
            smoke: false, image: plan.definition.runtime.buildImage, resultsDir: root },
        }))) };
    assert.deepEqual(validateCampaignAdmission(payload, plan, root), payload);
    const missing = { ...payload, reports: payload.reports.slice(1) };
    assert.throws(() => validateCampaignAdmission(missing, plan, root), /incomplete/);
    const changedLimit = structuredClone(payload);
    changedLimit.reports[0]!.request.maxOutputTokens = 1;
    assert.throws(() => validateCampaignAdmission(changedLimit, plan, root), /must contain one/);
    const substituted = structuredClone(payload);
    substituted.reports[0]!.request.providerRoute = 'another-provider';
    assert.throws(() => validateCampaignAdmission(substituted, plan, root), /must contain one/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('occupied-port scans yield to cancellation before exhausting the TCP range', async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-scan-cancel-'));
  const controller = new AbortController();
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    let probes = 0;
    await assert.rejects(runCampaignAdmission(plan, root, {
      signal: controller.signal, env: { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks') },
      probePort: () => {
        if (++probes === 1) setImmediate(() => controller.abort());
        return { free: false };
      },
      preflight: () => { throw new Error('must not reach preflight'); },
    }), { name: 'AbortError' });
    assert(probes > 0 && probes <= 16, `scan did ${probes} probes without yielding`);
    assert.equal(existsSync(join(root, 'admissions')), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('cancellation after reservation releases exact owned locks before admission returns',
  { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'campaign-preflight-cancel-'));
    const controller = new AbortController();
    const locks = join(root, 'locks');
    try {
      const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
      await assert.rejects(runCampaignAdmission(plan, root, {
        signal: controller.signal, env: { STACK_BENCH_RESOURCE_LOCK_DIR: locks },
        probePort: () => ({ free: true }),
        preflight: request => { controller.abort(); return passingPreflight(request); },
      }), { name: 'AbortError' });
      assert.equal(readdirSync(locks).filter(name => name.endsWith('.lock.json')).length, 0);
      assert.equal(existsSync(join(root, 'admissions')), false);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
