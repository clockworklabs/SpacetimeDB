import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, initializeCampaignDirectory, readCampaignState, writeCampaignState }
  from '../src/campaigns/campaign-scheduler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const example = compileCampaignFile(join(STACK_BENCH_ROOT, 'appliance',
  'campaign.example.json'));

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
