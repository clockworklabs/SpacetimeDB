import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { attemptChecks } from '../../dashboard/dashboard-views.js';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, createCampaignState } from '../../src/campaigns/campaign-scheduler.js';
import { writeArtifact } from '../../src/evidence/artifacts.js';
import { createCheckEvidence } from '../../src/evidence/check-evidence.js';
import { EXAMPLE_CAMPAIGN, writeCampaign, writeRunEvidence } from '../fixtures/dashboard-fixture.js';

test('check details retain measured and unmeasured evidence without leaking credentials', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-check-evidence-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const claimed = claimNextAttempt(createCampaignState(plan), { admissionId: 'evidence-test' });
  assert.ok(claimed.claim);
  const directory = join(root, 'campaigns', 'evidence');
  writeCampaign(directory, plan, claimed.state);
  const output = join(directory, claimed.claim.output);
  writeRunEvidence(output, plan, claimed.claim.attempt, 0);
  const read = () => attemptChecks(root, 'evidence', claimed.claim!.attempt.id);
  assert.equal(read().checks[0]?.observations[0]?.status, 'PASS');
  const bundlePath = join(output, 'grading', 'bundle.json');
  const bundle = JSON.parse(readFileSync(bundlePath, 'utf8'));
  const criterion = bundle.payload.suites.fixture.features[0].criteria[0];
  for (const status of ['failed', 'blocked', 'inconclusive', 'harness_failure'] as const) {
    criterion.evidence = createCheckEvidence({ status, code: 'test.observation',
      phase: status === 'blocked' ? 'setup' : 'assertion', startedAtMs: 1, completedAtMs: 2,
      summary: 'authorization: Bearer secret-summary', expected: { orders: 1 },
      observation: { orders: 2, detail: 'authorization: Bearer secret-observation' } });
    writeArtifact(bundlePath, bundle);
    const observed = read().checks[0]?.observations[0];
    assert.equal(observed?.status, status === 'failed' ? 'FAIL' : status.replaceAll('_', ' ').toUpperCase());
    assert.match(observed?.expected ?? '', /"orders": 1/);
    assert.match(observed?.actual ?? '', /"orders": 2/);
    assert.doesNotMatch(JSON.stringify(observed), /secret-summary|secret-observation/);
  }
  criterion.evidence.sensitivity = ['credentials'];
  criterion.evidence.summary = 'unlabelled-sensitive-value';
  writeArtifact(bundlePath, bundle);
  assert.deepEqual(read().checks[0]?.observations[0], {
    status: 'HARNESS FAILURE', summary: 'Sensitive evidence omitted.', expected: null, actual: null,
  });
  criterion.evidence.sensitivity = [];
  criterion.evidence.observation = 'x'.repeat(20_000);
  writeArtifact(bundlePath, bundle);
  const large = read().checks[0]?.observations[0]?.actual;
  assert.ok(large && large.length < 13_000);
  assert.match(large, /Truncated. Full evidence is in Files/);
  // A missing later grade must not erase earlier evidence or become a pass.
  writeArtifact(join(output, 'l1-fix1-grading', 'bundle.json'), bundle);
  rmSync(join(output, 'l1-fix1-grading', 'bundle.json'));
  const incomplete = read();
  assert.equal(incomplete.grades[1]?.error, 'grade bundle is missing');
  assert.equal(incomplete.checks[0]?.observations.length, 2);
  assert.equal(incomplete.checks[0]?.observations[1], null);
});
