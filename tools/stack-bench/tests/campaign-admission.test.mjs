import assert from 'node:assert/strict';
import test from 'node:test';

import { campaignAdmissionSmokeReuse }
  from '../src/campaigns/campaign-admission.mjs';

const createdAt = '2026-08-27T12:00:00.000Z';
const admission = {
  ok: true,
  createdAt,
  reports: [{
    ok: true,
    request: { agentAdapter: 'claude-code', runIndex: 2,
      backends: ['mongodb', 'postgres', 'spacetime'], image: 'stack-bench:fixed' },
    checks: [{ id: 'smoke.container', status: 'pass', summary: 'passed' }],
  }],
};
const request = { agentAdapter: 'claude-code', runIndex: 2,
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
  missing.reports[0].checks = [{ id: 'docker.engine', status: 'pass', summary: 'passed' }];
  const result = campaignAdmissionSmokeReuse(missing, request,
    { now: Date.parse(createdAt) });
  assert.equal(result.reusable, false);
  assert.match(result.reason, /no passing container smoke/);
});
