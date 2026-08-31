import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { campaignAdmissionSmokeReuse, runCampaignAdmission }
  from '../src/campaigns/campaign-admission.js';
import type { CampaignAdmissionSmokeInput, CampaignAdmissionSmokeRequest }
  from '../src/campaigns/campaign-admission.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAdmissionPreflightRequest }
  from '../src/campaigns/campaign-admission.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const createdAt = '2026-08-27T12:00:00.000Z';
const admission: CampaignAdmissionSmokeInput = {
  ok: true,
  createdAt,
  reports: [{
    ok: true,
    request: { agentAdapter: 'claude-code', runIndex: 2,
      backends: ['mongodb', 'postgres', 'spacetime'], image: 'stack-bench:fixed' },
    checks: [{ id: 'smoke.container', status: 'pass', summary: 'passed' }],
  }],
};
const request: CampaignAdmissionSmokeRequest = { agentAdapter: 'claude-code', runIndex: 2,
  backend: 'postgres', image: 'stack-bench:fixed' };

test('campaign smoke reuse requires an exact recent passing admission', () => {
  const recent = campaignAdmissionSmokeReuse(admission, request,
    { now: Date.parse(createdAt) + 60_000 });
  assert.deepEqual(recent, { reusable: true, reason: null, createdAt });

  const stale = campaignAdmissionSmokeReuse(admission, request,
    { now: Date.parse(createdAt) + 16 * 60_000 });
  assert.equal(stale.reusable, false);
  assert.match(stale.reason, /not recent/);

  const changedImage = campaignAdmissionSmokeReuse(admission,
    { ...request, image: 'stack-bench:changed' }, { now: Date.parse(createdAt) });
  assert.equal(changedImage.reusable, false);
  assert.match(changedImage.reason, /image changed/);

  assert.throws(() => campaignAdmissionSmokeReuse(admission,
    { ...request, backend: 'unknown' }, { now: Date.parse(createdAt) }), /does not cover stack/);
  assert.throws(() => campaignAdmissionSmokeReuse({ ...admission, ok: false }, request,
    { now: Date.parse(createdAt) }), /did not pass/);
});

test('campaign smoke reuse requires the real container smoke result', () => {
  const missing = structuredClone(admission);
  missing.reports[0]!.checks = [{ id: 'docker.engine', status: 'pass', summary: 'passed' }];
  const result = campaignAdmissionSmokeReuse(missing, request,
    { now: Date.parse(createdAt) });
  assert.equal(result.reusable, false);
  assert.match(result.reason, /no passing container smoke/);
});

test('campaign admission receives only the feature catalog levels in the compiled plan', () => {
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
    const result = runCampaignAdmission(plan, root, {
      now: createdAt,
      uuid: () => 'scoped',
      env: {},
      preflight: request => {
        requests.push(request);
        return {
          schemaVersion: 1,
          generatedAt: createdAt,
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, parallelism: request.parallelism,
            agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true,
          summary: { passed: 1, failed: 0, warnings: 0 },
          checks: [{ id: 'smoke.container', status: 'pass', summary: 'passed' }],
        };
      },
    });

    assert.equal(result.payload.ok, true);
    assert.equal(requests.length, plan.summary.parallelism);
    assert(requests.every(request => request.featureCatalog!.definition.nodes
      .every(node => node.level <= 3)));
    assert(plan.featureCatalog);
    const identity = plan.featureCatalog.identity;
    assert(requests.every(request => request.featureCatalog!.identity.sha256 === identity.sha256));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
