import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { campaignLockIsActive, controllerInstance, campaignLockTransaction,
  campaignCancellationRequested, watchCampaignCancellation, ownerAlive } from '../src/campaigns/campaign-lock.js';
import type { CampaignIdentity, CampaignLock, CampaignLockRecord } from '../src/campaigns/campaign-lock.js';

const campaign = { id: 'ecommerce-l1-example', contentSha256: 'a'.repeat(64) };

// These tests exercise transaction invariants on every source-development platform.
// Native process exclusion is exercised separately on Linux under real flock.
function acquireCampaignLock(root: string, identity: CampaignIdentity,
  { ownerPid = process.pid, ownerInstance = 'test-controller', uuid = randomUUID,
    alive = () => false }: { ownerPid?: number; ownerInstance?: string; uuid?: () => string;
      alive?: (record: Pick<CampaignLockRecord, 'ownerInstance' | 'ownerPid'>, current: string) => boolean } = {}): CampaignLock {
  const token = uuid();
  const lock: CampaignLock = { path: join(root, '.campaign.lock.json'), token,
    record: { version: 2, campaignId: identity.id, campaignSha256: identity.contentSha256,
      ownerPid, ownerInstance, ownershipMarkerSha256: createHash('sha256').update(token).digest('hex'),
      acquiredAt: new Date().toISOString() } };
  campaignLockTransaction({ operation: 'acquire', lock }, { alive });
  return lock;
}
const releaseCampaignLock = (lock: CampaignLock): boolean =>
  campaignLockTransaction({ operation: 'release', lock });

test('controller identity prefers explicit configuration over container and host identity', () => {
  const id = 'ab'.repeat(32);
  const containerMount = [
    '1044 997 0:81 / / rw,relatime - overlay overlay rw',
    `1051 1044 0:70 /docker/containers/${id}/hostname /etc/hostname rw,relatime - tmpfs tmpfs rw`,
    '1052 1044 0:70 /docker/containers/not-an-id/hostname /etc/hostname rw - tmpfs tmpfs rw',
  ].join('\n');
  const noProcfs = (): string => { throw Object.assign(new Error('no procfs'), { code: 'ENOENT' }); };
  for (const [env, readMountInfo, fallback, expected] of [
    [{ STACK_BENCH_CONTROLLER_INSTANCE: '  controller-explicit  ' }, null, 'unused-host', 'controller-explicit'],
    [{}, () => containerMount, 'docker-desktop', id],
    [{}, noProcfs, 'host-controller', 'host-controller'],
    [{}, () => '/docker/containers/short-id/hostname /etc/hostname', 'host-controller', 'host-controller'],
  ] as const) {
    let mountRead = false;
    assert.equal(controllerInstance(env, {
      readMountInfo: () => { mountRead = true; return readMountInfo ? readMountInfo() : 'unused'; },
      fallbackHostname: () => fallback,
    }), expected);
    assert.equal(mountRead, readMountInfo !== null);
  }
});

test('cross-controller liveness fails closed when Docker cannot answer', () => {
  const container = 'ab'.repeat(32);
  const record = { ownerInstance: container, ownerPid: 14 };
  assert.throws(() => ownerAlive(record, 'cd'.repeat(32), {
    inspect: () => ({ error: new Error('docker unavailable'), status: null }),
  }), /cannot inspect controller/);
  assert.throws(() => ownerAlive(record, 'cd'.repeat(32), {
    inspect: () => ({ status: 1, stdout: '', stderr: 'permission denied' }),
  }), /cannot determine whether controller/);
});

test('cross-controller liveness reclaims only a stopped or missing exact container', () => {
  const container = 'ab'.repeat(32);
  const record = { ownerInstance: container, ownerPid: 14 };
  assert.equal(ownerAlive(record, 'cd'.repeat(32), {
    inspect: () => ({ status: 0, stdout: 'true\n', stderr: '' }),
  }), true);
  assert.equal(ownerAlive(record, 'cd'.repeat(32), {
    inspect: () => ({ status: 0, stdout: 'false\n', stderr: '' }),
  }), false);
  assert.equal(ownerAlive(record, 'cd'.repeat(32), {
    inspect: () => ({ status: 1, stdout: '', stderr: `Error: No such object: ${container}` }),
  }), false);
});

test('a campaign lock admits one exact controller and only its token can release it', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-'));
  try {
    const lock = acquireCampaignLock(root, campaign, { ownerInstance: 'controller-a',
      uuid: () => 'owner-token', alive: () => true });
    for (const ownerInstance of ['controller-a', 'controller-b']) {
      assert.throws(() => acquireCampaignLock(root, campaign, { ownerInstance,
        alive: record => record.ownerInstance === 'controller-a' }),
      new RegExp(`already controlled by controller-a pid ${process.pid}`));
    }
    assert.throws(() => releaseCampaignLock({ ...lock, token: 'wrong-token' }), /token does not match|no longer belongs/);
    assert.equal(releaseCampaignLock(lock), true);
    assert.equal(releaseCampaignLock(lock), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('dead-owner reclamation preserves campaign binding and rejects the old releaser', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-stale-'));
  try {
    const first = acquireCampaignLock(root, campaign, { ownerPid: 1001,
      ownerInstance: 'dead-controller', uuid: () => 'first-token', alive: () => false });
    const inspected: Array<{ record: { ownerInstance: string; ownerPid: number }; current: string }> = [];
    const second = acquireCampaignLock(root, campaign, { ownerPid: 1002,
      ownerInstance: 'new-controller',
      uuid: (() => { let index = 0; return () => `second-token-${++index}`; })(),
      alive: (record, current) => { inspected.push({ record, current }); return false; } });
    assert.equal(inspected[0]?.record.ownerInstance, 'dead-controller');
    assert.equal(inspected[0]?.current, 'new-controller');
    const record = JSON.parse(readFileSync(second.path, 'utf8'));
    assert.equal(record.ownerPid, 1002);
    assert.equal(record.campaignSha256, campaign.contentSha256);
    assert.throws(() => releaseCampaignLock(first), /no longer belongs/);
    assert.equal(releaseCampaignLock(second), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign lock liveness is read without changing stale ownership evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-lock-status-'));
  try {
    assert.equal(campaignLockIsActive(root, campaign, { currentInstance: 'dashboard',
      alive: () => true }), false);
    const lock = acquireCampaignLock(root, campaign, { ownerPid: 14,
      ownerInstance: 'campaign-controller', uuid: () => 'campaign-token', alive: () => false });
    assert.equal(campaignLockIsActive(root, campaign, { currentInstance: 'dashboard',
      alive: record => record.ownerInstance === 'campaign-controller' }), true);
    assert.equal(campaignLockIsActive(root, campaign, { currentInstance: 'dashboard',
      alive: () => false }), false);
    assert.throws(() => campaignLockIsActive(root,
      { ...campaign, contentSha256: 'b'.repeat(64) }, { alive: () => true }), /does not belong/);
    assert.equal(releaseCampaignLock(lock), true);
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


test('cancellation belongs to one lock generation and a stale Stop cannot cancel its replacement', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-cancel-'));
  try {
    const first = acquireCampaignLock(root, campaign);
    const cancel = (lock: CampaignLock): boolean => campaignLockTransaction({ operation: 'cancel',
      path: lock.path, campaign, ownershipMarkerSha256: lock.record.ownershipMarkerSha256,
      requestedAt: new Date().toISOString() });
    const watcher = watchCampaignCancellation(first);
    try {
      assert.equal(cancel(first), true);
      assert.equal(cancel(first), true);
      assert.equal(campaignCancellationRequested(first), true);
      await new Promise(resolve => setTimeout(resolve, 300));
      assert.equal(watcher.signal.aborted, true);
    } finally { watcher.close(); }
    const second = acquireCampaignLock(root, campaign);
    assert.equal(cancel(first), false);
    assert.equal(campaignCancellationRequested(second), false);
    const before = readFileSync(second.path, 'utf8');
    assert.throws(() => releaseCampaignLock(first), /no longer belongs/);
    assert.equal(readFileSync(second.path, 'utf8'), before);
    assert.equal(cancel(second), true);
    assert.equal(campaignCancellationRequested(second), true);
    releaseCampaignLock(second);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('external cancellation reaches the owner watcher immediately', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-cancel-signal-'));
  try {
    const lock = acquireCampaignLock(root, campaign);
    const source = new AbortController();
    const watcher = watchCampaignCancellation(lock, source.signal);
    try { source.abort(); assert.equal(watcher.signal.aborted, true); }
    finally { watcher.close(); releaseCampaignLock(lock); }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
