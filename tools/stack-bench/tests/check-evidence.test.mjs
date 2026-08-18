import assert from 'node:assert/strict';
import test from 'node:test';

import {
  CHECK_EVIDENCE_DISPOSITIONS,
  createCheckEvidence,
  criterionEvidence,
  evidenceDisposition,
  evidenceIsApplicationFailure,
  evidenceIsMeasured,
  evidenceIsRepairable,
  evidencePassed,
  validateCheckEvidence,
} from '../src/evidence/check-evidence.mjs';
import {
  evidenceStatusLabel,
  renderEvidenceConsoleLine,
  renderRepairDiagnostic,
} from '../src/evidence/evidence-presentation.mjs';

function evidence(overrides = {}) {
  return createCheckEvidence({
    status: 'failed',
    code: 'application_failure',
    phase: 'assertion',
    summary: 'the message is presentation, not protocol',
    startedAtMs: 10,
    completedAtMs: 15,
    ...overrides,
  });
}

test('check evidence validates typed status', () => {
  const value = evidence({ status: 'harness_failure', code: 'browser_failure' });
  assert.equal(validateCheckEvidence(value), value);
  assert.equal(evidenceIsMeasured(value), false);
});

test('criterion verdict ignores wording when typed evidence exists', () => {
  const criterion = {
    id: 'works',
    evidence: evidence({ status: 'passed', code: 'completed', summary: 'anything at all' }),
  };
  assert.equal(criterionEvidence(criterion).status, 'passed');
});

test('criteria without typed evidence are rejected', () => {
  assert.throws(() => criterionEvidence({ id: 'old', passed: false,
    detail: 'INCONCLUSIVE: unavailable' }), /evidence is required/);
});

test('unknown fields and inconsistent timing are rejected', () => {
  const unknown = { ...evidence(), surprise: true };
  assert.throws(() => validateCheckEvidence(unknown), /surprise is unknown/);
  const badTiming = evidence();
  badTiming.timing.durationMs = 99;
  assert.throws(() => validateCheckEvidence(badTiming), /does not match/);
});

test('one immutable status table owns verdict, outcome and repair semantics', () => {
  assert.deepEqual(Object.fromEntries(Object.entries(CHECK_EVIDENCE_DISPOSITIONS)
    .map(([status, disposition]) => [status, {
      label: disposition.label,
      outcomeKind: disposition.outcomeKind,
      passed: disposition.passed,
      measured: disposition.measured,
      repairable: disposition.repairable,
    }])), {
    passed: { label: 'PASS', outcomeKind: 'passed', passed: true, measured: true, repairable: false },
    failed: { label: 'FAIL', outcomeKind: 'app_failure', passed: false, measured: true, repairable: true },
    inconclusive: { label: 'INCONCLUSIVE', outcomeKind: 'inconclusive', passed: false,
      measured: false, repairable: false },
    harness_failure: { label: 'HARNESS FAILURE', outcomeKind: 'harness_failure', passed: false,
      measured: false, repairable: false },
  });
  assert.ok(Object.isFrozen(CHECK_EVIDENCE_DISPOSITIONS));
  assert.ok(Object.values(CHECK_EVIDENCE_DISPOSITIONS).every(Object.isFrozen));
});

test('all semantic helpers and renderers obey typed status, never diagnostic wording', () => {
  const misleading = evidence({ status: 'failed', summary: 'PASS: everything is wonderful' });
  assert.equal(evidenceDisposition(misleading).outcomeKind, 'app_failure');
  assert.equal(evidencePassed(misleading), false);
  assert.equal(evidenceIsMeasured(misleading), true);
  assert.equal(evidenceIsApplicationFailure(misleading), true);
  assert.equal(evidenceIsRepairable(misleading), true);
  assert.equal(evidenceStatusLabel(misleading), 'FAIL');
  assert.match(renderEvidenceConsoleLine(misleading, 'feature/check'), /^FAIL feature\/check/);
  assert.match(renderRepairDiagnostic(misleading), /PASS: everything is wonderful/);

  const unavailable = evidence({ status: 'harness_failure', code: 'browser_failure',
    summary: 'FAILED: blame the generated app' });
  assert.equal(evidenceDisposition(unavailable).outcomeKind, 'harness_failure');
  assert.equal(evidenceIsMeasured(unavailable), false);
  assert.equal(evidenceIsRepairable(unavailable), false);
  assert.equal(evidenceStatusLabel(unavailable), 'HARNESS FAILURE');
  assert.throws(() => renderRepairDiagnostic(unavailable), /cannot render a repair diagnostic/);
});

test('unknown status cannot fall through to a default classification', () => {
  assert.throws(() => evidenceDisposition('mystery'), /status is invalid/);
  assert.throws(() => evidenceStatusLabel({}), /status is invalid/);
});
