import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runCampaignAdmission }
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
    packs: request.packIds, checks: request.checkKeys, image: request.image,
    resultsDir: request.resultsDir, smoke: request.smoke },
  ok: true,
  summary: { passed: 1, failed: 0, warnings: 0 },
  checks: [{ id: 'smoke.container', status: 'pass' as const, summary: 'passed' }],
});

test('admission rejects campaign concurrency above the runner pool before resource preflight', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-capacity-admission-'));
  try {
    const example = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'tests', 'fixtures',
      'campaign.deterministic.json'), 'utf8'));
    const campaignPath = join(root, 'campaign.json');
    writeFileSync(campaignPath, JSON.stringify({ ...example, parallelism: 2 }));
    const plan = compileCampaignFile(campaignPath);
    assert.throws(() => runCampaignAdmission(plan, root, {
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks'), STACK_BENCH_RUNNER_CAPACITY: '1' },
      preflight: () => { throw new Error('preflight must not start'); } }), /exceeds declared runner capacity/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign admission receives only the feature catalog levels in the compiled plan', { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false }, () => {
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
    const result = runCampaignAdmission(plan, root, {       now: createdAt,
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

test('campaign admission selects a free run slot', { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false }, () => {
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
    const result = runCampaignAdmission(plan, root, {       now: createdAt,
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
