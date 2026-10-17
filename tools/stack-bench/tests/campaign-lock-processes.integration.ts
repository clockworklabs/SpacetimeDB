import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { acquireCampaignLock, readCampaignLock, releaseCampaignLock,
  requestCampaignCancellation } from '../src/campaigns/campaign-lock.js';
import { compiledEntrypoint } from '../src/package-root.js';

const campaign = { id: 'native-lock-proof', contentSha256: 'a'.repeat(64) };
const linux = { skip: process.platform !== 'linux' ? 'Real campaign control requires the Linux Docker appliance and flock' : false };

test('native campaign lock excludes contenders and a durable Stop survives requester exit', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-campaign-'));
  const helper = compiledEntrypoint('tests', 'fixtures', 'campaign-lock-process.js');
  const child = spawn(process.execPath, [helper, root], { stdio: ['ignore', 'pipe', 'pipe'] });
  const exited = new Promise<number | null>((resolve, reject) => {
    child.once('error', reject); child.once('exit', resolve);
  });
  try {
    await new Promise<void>((resolve, reject) => {
      let stderr = '';
      child.stderr.on('data', chunk => { stderr += chunk; });
      child.stdout.once('data', () => resolve());
      child.once('exit', code => reject(new Error(`owner exited ${code}: ${stderr}`)));
      child.once('error', reject);
    });
    const before = readCampaignLock(root)!;
    assert.throws(() => acquireCampaignLock(root, campaign), /already controlled/);
    assert.deepEqual(readCampaignLock(root), before);
    assert.equal(requestCampaignCancellation(root, campaign, before.ownershipMarkerSha256), true);
    assert.equal(await exited, 0);
    assert.equal(readCampaignLock(root), null);
    const replacement = acquireCampaignLock(root, campaign);
    assert.equal(requestCampaignCancellation(root, campaign, before.ownershipMarkerSha256), false);
    const bytes = readFileSync(replacement.path, 'utf8');
    assert.throws(() => releaseCampaignLock({ ...replacement, token: 'wrong' }), /token does not match/);
    assert.equal(readFileSync(replacement.path, 'utf8'), bytes);
    releaseCampaignLock(replacement);
  } finally {
    if (child.exitCode === null) { child.kill('SIGKILL'); await exited; }
    rmSync(root, { recursive: true, force: true });
  }
});

test('concurrent native claimers cannot steal a live replacement while reclaiming a dead owner', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-campaign-race-'));
  const helper = compiledEntrypoint('tests', 'fixtures', 'campaign-lock-process.js');
  // This pid is outside Linux pid_max on the appliance.
  acquireCampaignLock(root, campaign, { ownerPid: 2147483647 });
  const children = Array.from({ length: 8 }, () => spawn(process.execPath,
    [helper, root, 'race'], { stdio: ['ignore', 'pipe', 'pipe'] }));
  try {
    let rejected = 0;
    const codes = await Promise.all(children.map(child => new Promise<number | null>((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', code => {
        if (code === 2 && ++rejected === 7) {
          const owner = readCampaignLock(root)!;
          requestCampaignCancellation(root, campaign, owner.ownershipMarkerSha256);
        }
        resolve(code);
      });
    })));
    assert.equal(codes.filter(code => code === 0).length, 1);
    assert.equal(codes.filter(code => code === 2).length, 7);
  } finally {
    for (const child of children) if (child.exitCode === null) child.kill('SIGKILL');
    rmSync(root, { recursive: true, force: true });
  }
});
