import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { referenceRuns, referenceLogPoints } from '../../dashboard/dashboard-reference-runs.js';
import { campaignsPage } from '../../dashboard/public/views/campaigns.js';

test('standalone reference runs use real controller state, keep the denominator and survive controller removal', async () => {
  const root = mkdtempSync(join(tmpdir(), 'reference-dashboard-'));
  try {
    const directory = join(root, 'reference-live'); mkdirSync(directory);
    const id = 'a'.repeat(64), output = join(directory, 'convex.json');
    let running = true, present = true;
    const log = 'qualifying convex: clean run 1/1\n  scope: 117 check(s), 186 point(s)\n'
      + '  selected-source-001 ... 1/1\n  selected-source-002 ... 0/2\n  selected-source-003 ... not selected\n';
    const docker = async (args: string[]) => {
      if (args[0] === 'ps') return present ? JSON.stringify({ ID: id, Command: 'node /opt/stack-bench/dist/src/references/reference-live.js' }) : '';
      if (args[0] === 'inspect') return JSON.stringify([
        { Id: id, Args: ['--out', output, '--backend', 'convex', '--level', '3'],
          State: { Running: running, StartedAt: '2026-09-21', FinishedAt: '2026-09-22' } },
        { Id: 'b'.repeat(64), Args: ['--out', join(root, 'outside.json')], State: { Running: true } },
      ]);
      assert.equal(args.at(-1), id); return log;
    };
    const first = await referenceRuns(root, docker);
    assert.equal(first.runs.length, 1);
    assert.equal(first.runs[0]!.status, 'running');
    assert.deepEqual(first.runs[0]!.points, { passed: 1, measured: 3, planned: 186 });
    assert.match(campaignsPage({ campaigns: [], sheets: [], filter: 'all', references: first }), /1\/186 points passed · 3 measured/);
    assert.match(campaignsPage({ campaigns: [], sheets: [], filter: 'all', references: { runs: [
      { ...first.runs[0]!, points: { passed: 1, measured: 3, planned: null } }], error: null } }), /total unavailable/);
    running = false;
    assert.equal((await referenceRuns(root, docker)).runs[0]!.status, 'incomplete');
    writeFileSync(output, JSON.stringify({ payload: { kind: 'reference_qualification', fixture: 'convex',
      ok: false, runs: [{ score: '184/186', failures: ['<failure>'] }], completedAt: '2026-09-22' } }));
    assert.equal((await referenceRuns(root, docker)).runs[0]!.status, 'failed');
    present = false;
    const final = await referenceRuns(root, docker);
    assert.equal(final.runs[0]!.status, 'failed');
    assert.deepEqual(final.runs[0]!.points, { passed: 184, measured: 186, planned: 186 });
    assert.match(campaignsPage({ campaigns: [], sheets: [], filter: 'all', references: final }), /&lt;failure&gt;/);
    assert((await referenceRuns(root, async () => { throw new Error('offline'); })).error);
    assert.deepEqual(referenceLogPoints(log + 'qualifying convex: clean run 2/2\n  selected-source-001 ... 1/1\n'),
      { passed: 1, measured: 1, planned: null });
  } finally { rmSync(root, { recursive: true, force: true }); }
});
