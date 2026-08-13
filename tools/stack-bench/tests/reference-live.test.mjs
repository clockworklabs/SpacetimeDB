import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { auditReferenceRun, parseReferenceQualificationArgs, referenceQualificationContext,
  referenceQualificationPaths, referenceQualificationWorkRoot, rescueSupervisedLease,
  runBounded } from '../reference-live.mjs';
import { writeArtifact, writeRunJson } from '../artifacts.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const fixture = { backend: 'mongodb', track: 'ecommerce', level: 1,
  imported: { sourceSha256: 'a'.repeat(64) } };

test('reference qualification requires an explicit valid stack scope', () => {
  const args = parseReferenceQualificationArgs(['node', 'reference-live.mjs', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '2']);
  assert.equal(args.track, 'ecommerce');
  assert.equal(args.level, 2);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '3']), /validated/);
});

test('reference qualification resolves the exact executable calibration identity', () => {
  const context = referenceQualificationContext({ ...fixture, id: 'ecommerce-l1-mongodb',
    imported: { sourceSha256: 'd746c28f4e31a3de93211296d8fc4e3fd7b5a52c1cb09a175e4ec1d44fade73a' } });
  assert.equal(context.identity.id, 'ecommerce.l1-standard-calibration');
  assert.equal(context.identity.sha256, context.calibration.qualificationSha256);
});

test('reference qualification keeps underlying runs beside the requested artifact', () => {
  const root = join(tmpdir(), 'stack-bench-reference-output-test');
  const paths = referenceQualificationPaths({ out: join(root, 'postgres-reference.json') }, 'ignored-id');
  assert.equal(paths.artifactPath, join(root, 'postgres-reference.json'));
  assert.equal(paths.artifactDirectory, root);
  assert.equal(paths.runsRoot, join(root, 'postgres-reference.runs'));
});

test('reference qualification uses the daemon-visible appliance work root', () => {
  assert.equal(referenceQualificationWorkRoot({ STACK_BENCH_WORK_DIR: '/var/lib/stack-bench/work' }),
    resolve('/var/lib/stack-bench/work'));
});

function writeEvidence(root, { id, points, passed }) {
  mkdirSync(join(root, 'grading'), { recursive: true });
  writeRunJson(join(root, 'run.json'), {
    id: 'reference-run', backend: 'mongodb', track: 'ecommerce',
    setup: { isolation: { mode: 'container', imageId: 'sha256:test' } },
    outcome: { kind: 'passed' },
    levels: [{ level: 1, graded: true, contractPass: true, score: 1, max: 1 }],
    backendLease: { state: 'released', resources: {
      buildContainer: { running: false }, locks: [{ key: 'slot:test', releasedAt: 'now' }],
    } },
  });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const evidence = createCheckEvidence({ status: passed ? 'passed' : 'failed',
    code: passed ? 'completed' : 'test_result', phase: 'assertion',
    startedAtMs: 1, completedAtMs: 2 });
  writeArtifact(join(root, 'grading', 'bundle.json'), { kind: 'grade_bundle', id: 'reference-bundle',
    payload: { suites: {
      lint: { pass: true },
      systems: { features: [{ id: 901, setupEvidence, criteria: [{ id, points, evidence }] }] },
    } } });
}

test('reference qualification audits zero-point criteria and teardown evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    writeEvidence(root, { id: '901a', points: 0, passed: true });
    const audit = auditReferenceRun(root, fixture);
    assert.equal(audit.ok, true);
    assert.equal(audit.criteria, 1);
    assert.equal(audit.zeroPointCriteria, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification rejects a failed zero-point criterion', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    writeEvidence(root, { id: '901a', points: 0, passed: false });
    const audit = auditReferenceRun(root, fixture);
    assert.equal(audit.ok, false);
    assert.deepEqual(audit.failures, ['systems/901/901a did not pass']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('mutation qualification requires a full baseline and every exact mutant caught', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    writeEvidence(root, { id: '901a', points: 0, passed: true });
    const runPath = join(root, 'run.json');
    const run = JSON.parse(readFileSync(runPath, 'utf8'));
    run.payload.mutationControl = { ok: true };
    writeFileSync(runPath, JSON.stringify(run));
    writeArtifact(join(root, 'mutation-control.json'), { kind: 'mutation_control', id: 'mutation-run',
      payload: { ok: true, fixtureSha256: fixture.imported.sourceSha256,
        baseline: { total: 2, max: 2 }, summary: { caught: 1, total: 1 },
        results: [{ id: 'known-defect', status: 'CAUGHT' }] } });
    const passing = auditReferenceRun(root, fixture, { requireMutationControl: true });
    assert.equal(passing.ok, true);
    assert.deepEqual(passing.mutations, { caught: 1, total: 1 });

    const failed = JSON.parse(readFileSync(join(root, 'mutation-control.json'), 'utf8'));
    failed.payload.results[0].status = 'SURVIVED';
    writeFileSync(join(root, 'mutation-control.json'), JSON.stringify(failed));
    const audit = auditReferenceRun(root, fixture, { requireMutationControl: true });
    assert.equal(audit.ok, false);
    assert(audit.failures.includes('known-defect is SURVIVED'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification terminates a child at its repetition deadline', async () => {
  const started = Date.now();
  const result = await runBounded(process.execPath,
    ['-e', 'setInterval(() => {}, 1000)'], {
      stdio: 'ignore', timeoutMs: 50,
      terminate: pid => process.kill(pid, 'SIGKILL'),
    });
  assert.equal(result.ok, false);
  assert.equal(result.timedOut, true);
  assert(Date.now() - started < 10_000, 'timed-out child was not terminated promptly');
});

test('supervisor accepts a deleted private lease only with matching released evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-supervisor-evidence-'));
  try {
    const state = join(root, 'supervisor.json');
    const output = join(root, 'output');
    mkdirSync(output);
    const runtimeDir = join(root, 'runtime');
    writeFileSync(state, JSON.stringify({ version: 2, runId: 'released-run', backend: 'mongodb',
      runtimeDir, leasePath: join(runtimeDir, 'backend-lease.json'),
      ownershipToken: 'private-token', output }));
    writeRunJson(join(output, 'run.json'), { id: 'released-run', backendLease: {
      runId: 'released-run', state: 'released', resources: {
        buildContainer: { running: false }, locks: [{ releasedAt: 'now' }],
      },
    } });
    assert.doesNotThrow(() => rescueSupervisedLease(state, output));
    const run = JSON.parse(readFileSync(join(output, 'run.json'), 'utf8'));
    run.payload.backendLease.state = 'active';
    writeFileSync(join(output, 'run.json'), JSON.stringify(run));
    assert.throws(() => rescueSupervisedLease(state, output), /without released run evidence/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
