import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { acquireCampaignLock, releaseCampaignLock } from '../campaign-lock.mjs';

const campaign = { id: 'ecommerce-l1-example', contentSha256: 'a'.repeat(64) };

test('a campaign lock admits one exact controller and only its token can release it', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-'));
  try {
    const lock = acquireCampaignLock(root, campaign, { uuid: () => 'owner-token' });
    assert.throws(() => acquireCampaignLock(root, campaign), /already controlled/);
    assert.throws(() => releaseCampaignLock({ ...lock, token: 'wrong-token' }), /no longer belongs/);
    assert.equal(releaseCampaignLock(lock), true);
    assert.equal(releaseCampaignLock(lock), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('dead-owner reclamation moves only the exact stale lock and preserves campaign binding', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-stale-'));
  try {
    const first = acquireCampaignLock(root, campaign, { ownerPid: 1001,
      uuid: () => 'first-token', alive: () => false });
    const second = acquireCampaignLock(root, campaign, { ownerPid: 1002,
      uuid: (() => { let index = 0; return () => `second-token-${++index}`; })(),
      alive: () => false });
    const record = JSON.parse(readFileSync(second.path, 'utf8'));
    assert.equal(record.ownerPid, 1002);
    assert.equal(record.campaignSha256, campaign.contentSha256);
    assert.throws(() => releaseCampaignLock(first), /no longer belongs/);
    assert.equal(releaseCampaignLock(second), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('malformed ownership evidence is quarantined rather than guessed or stolen', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-bad-'));
  try {
    const path = join(root, '.campaign.lock.json');
    writeFileSync(path, '{"ownerPid":123}\n');
    assert.throws(() => acquireCampaignLock(root, campaign, { alive: () => false }),
      /required|unsupported/);
    assert.equal(readFileSync(path, 'utf8'), '{"ownerPid":123}\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
