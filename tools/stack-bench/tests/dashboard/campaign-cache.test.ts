import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { campaignSheet, campaignProgression } from '../../dashboard/dashboard-views.js';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import { createCampaignState, claimNextAttempt } from '../../src/campaigns/campaign-scheduler.js';
import { discoverPlans } from '../../dashboard/dashboard-model.js';
import { EXAMPLE_CAMPAIGN, writeCampaign, writePlanFixtures } from '../fixtures/dashboard-fixture.js';

test('dashboard shares validated plan/state until either file changes, but checks liveness each read', t => {
  const root = fs.mkdtempSync(join(tmpdir(), 'dashboard-campaign-cache-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const directory = join(root, 'campaigns', 'sample');
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const { state } = claimNextAttempt(createCampaignState(plan), { admissionId: 'test' });
  writeCampaign(directory, plan, state);
  const planPath = join(directory, 'plan.json');
  let planReads = 0;
  const originalRead = fs.readFileSync;
  t.mock.method(fs, 'readFileSync', (...args: Parameters<typeof fs.readFileSync>) => {
    if (String(args[0]) === planPath) planReads++;
    return originalRead(...args);
  });
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });

  assert.equal(campaignSheet(root, 'sample', { controllerActive: () => true }).status, 'running');
  campaignProgression(root, 'sample');
  assert.equal(campaignSheet(root, 'sample', { controllerActive: () => false }).status, 'attention-required');
  assert.equal(planReads, 1);

  const later = new Date(Date.now() + 5000);
  fs.utimesSync(join(directory, 'state.json'), later, later);
  campaignSheet(root, 'sample', { controllerActive: () => true });
  assert.equal(planReads, 2);
  fs.writeFileSync(planPath, '{}');
  assert.throws(() => campaignProgression(root, 'sample'));
  assert.equal(planReads, 3);
});

test('saved plans compile once until a plan file changes', t => {
  const root = fs.mkdtempSync(join(tmpdir(), 'dashboard-plan-cache-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  writePlanFixtures(root);
  const names = fs.readdirSync(root).filter(name => name.endsWith('.json'));
  const changed = names[0]!;
  let reads = 0;
  const originalRead = fs.readFileSync;
  t.mock.method(fs, 'readFileSync', (...args: Parameters<typeof fs.readFileSync>) => {
    if (names.some(name => String(args[0]).endsWith(name))) reads++;
    return originalRead(...args);
  });
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });

  const first = discoverPlans(root);
  assert.ok(reads >= names.length);
  reads = 0;
  assert.deepEqual(discoverPlans(root), first);
  assert.equal(reads, 0, 'an unchanged plan file is not read again');
  const later = new Date(Date.now() + 5000);
  fs.utimesSync(join(root, changed), later, later);
  discoverPlans(root);
  assert.ok(reads > 0 && reads <= 2, 'only the changed plan is compiled again');
  fs.rmSync(join(root, changed));
  assert.equal(discoverPlans(root).length, first.length - 1);
});
