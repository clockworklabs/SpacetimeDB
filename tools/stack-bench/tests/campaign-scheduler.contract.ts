import assert from 'node:assert/strict';
import fs, { mkdtempSync, readFileSync, rmSync, unlinkSync } from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile, validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, createCampaignState, initializeCampaignDirectory, readCampaignState, writeCampaignState }
  from '../src/campaigns/campaign-scheduler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const example = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures',
  'campaign.deterministic.json'));

test('state persistence validates current inputs once and still rejects tampering', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-state-validation-'));
  const path = join(root, 'state.json');
  const campaign = structuredClone(example);
  const state = createCampaignState(campaign);
  const originalRead = fs.readFileSync;
  let calibrationReads = 0;
  t.mock.method(fs, 'readFileSync', (...args: Parameters<typeof fs.readFileSync>) => {
    if (String(args[0]).replaceAll('\\', '/').includes('/composition/calibrations/')) calibrationReads++;
    return originalRead(...args);
  });
  syncBuiltinESMExports();
  try {
    validateCompiledCampaignPlan(campaign);
    const strictValidationReads = calibrationReads;
    assert(strictValidationReads > 0, 'the fixture must resolve current calibration inputs');
    calibrationReads = 0;
    writeCampaignState(path, campaign, state);
    const stateWriteReads = calibrationReads;
    t.mock.restoreAll();
    syncBuiltinESMExports();

    const saved = readFileSync(path, 'utf8');
    const artifact = JSON.parse(saved);
    assert.deepEqual(artifact.payload, state);
    assert.deepEqual(artifact.identities.experiment, {
      id: campaign.id, version: campaign.version, sha256: campaign.contentSha256, state: campaign.state,
    });
    assert.throws(() => writeCampaignState(path, { ...campaign, contentSha256: '0'.repeat(64) }, state),
      /content identity/);
    const changed = structuredClone(state);
    changed.attempts[0]!.plan.model = 'different-model';
    assert.throws(() => writeCampaignState(path, campaign, changed), /attempt plan/);
    assert.equal(readFileSync(path, 'utf8'), saved, 'rejected writes must preserve the last valid state');
    assert.equal(stateWriteReads, strictValidationReads,
      'persisting state must not resolve the same current calibration twice');
  } finally {
    t.mock.restoreAll();
    syncBuiltinESMExports();
    rmSync(root, { recursive: true, force: true });
  }
});

test('campaign directory initialization is identity-bound and resumes exact state', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-state-'));
  try {
    const campaign = structuredClone(example);
    const initialized = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const claimed = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: 'admission-1' });
    writeCampaignState(initialized.paths.state, campaign, claimed.state);
    const resumed = readCampaignState(root);
    assert.equal(resumed.plan.contentSha256, campaign.contentSha256);
    assert.equal(resumed.state.attempts[0]?.executions[0]?.status, 'running');
    assert.equal(initializeCampaignDirectory(campaign, root).state.summary.running, 1);
    assert.throws(() => initializeCampaignDirectory({ ...campaign,
      contentSha256: 'a'.repeat(64) }, root), /content identity/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('interrupted initialization recreates only missing state from the stored plan', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-init-recovery-'));
  try {
    const campaign = structuredClone(example);
    const initialized = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:00:00.000Z' });
    unlinkSync(initialized.paths.state);
    const recovered = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:01:00.000Z' });
    assert.equal(recovered.state.status, 'prepared');
    assert.equal(recovered.state.summary.executions, 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
