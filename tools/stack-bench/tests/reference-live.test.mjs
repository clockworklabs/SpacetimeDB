import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { auditReferenceRun, parseReferenceQualificationArgs, referenceQualificationContext,
  referenceQualificationPaths, referenceQualificationRunner, referenceQualificationSelectionArgs,
  referenceQualificationWorkRoot, rescueSupervisedLease, runBounded } from '../reference-live.mjs';
import { writeArtifact, writeRunJson } from '../artifacts.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';
import { createBoundRecipeTaskRequest } from '../recipe-selection.mjs';
import { resolveRecipeRelease } from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const fixture = { backend: 'mongodb', track: 'ecommerce', level: 1,
  imported: { sourceSha256: 'a'.repeat(64) } };

test('reference qualification requires an explicit valid stack scope', () => {
  const args = parseReferenceQualificationArgs(['node', 'reference-live.mjs', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '2']);
  assert.equal(args.track, 'ecommerce');
  assert.equal(args.level, 2);
  assert.equal(args.mutations, false);
  assert.equal(args.timeoutMinutes, 60);
  const mutationArgs = parseReferenceQualificationArgs(['node', 'reference-live.mjs', '--backend', 'postgres',
    '--mutations']);
  assert.equal(mutationArgs.mutations, true);
  assert.equal(mutationArgs.timeoutMinutes, 90);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.mjs', '--backend', 'postgres',
    '--mutations', '--timeout-minutes', '120']).timeoutMinutes, 120);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '3']).level, 3);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '4']), /declared/);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '1',
    '--recipe', 'ecommerce.l1-standard@1.1.0']).recipe, 'ecommerce.l1-standard@1.1.0');
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--repetitions', '1']).repetitions, 1);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.mjs',
    '--backend', 'postgres', '--repetitions', '0']), /positive integer/);
});

test('reference qualification resolves the exact executable calibration identity', () => {
  const context = referenceQualificationContext({ ...fixture, id: 'ecommerce-l1-direct-actions-mongodb',
    imported: { sourceSha256: 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40' } });
  assert.equal(context.identity.id, 'ecommerce.l1-modular-calibration');
  assert.equal(context.identity.sha256, context.calibration.qualificationSha256);
});

test('reference qualification resolves the requested promoted calibration', () => {
  const context = referenceQualificationContext({ ...fixture, id: 'ecommerce-l1-direct-actions-mongodb',
    imported: { sourceSha256: 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40' } },
  'ecommerce.l1-modular@2.3.0');
  assert.equal(context.binding.release.version, '2.3.0');
  assert.equal(context.calibration.version, '2.3.0');
  assert.equal(context.calibration.state, 'qualified');
});

test('modular reference qualification selects every exact check without prescribing specifications', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.3.0');
  const argv = referenceQualificationSelectionArgs(binding);
  const valueAfter = flag => argv[argv.indexOf(flag) + 1].split(',');
  const featureIds = valueAfter('--feature-module');
  const expectedSpecifications = valueAfter('--expect-spec');
  const checkKeys = valueAfter('--check');

  assert.equal(argv.includes('--request-spec'), false);
  assert.equal(checkKeys.length, 48);
  assert.equal(new Set(checkKeys).size, 48);
  const task = createBoundRecipeTaskRequest(binding,
    { featureIds, expectedSpecifications, checkKeys });
  assert.equal(task.selection.checks.length, 48);
  assert.equal(task.selection.scoredPoints, 58);
  assert.equal(task.selection.checks.filter(check => check.points === 0).length, 2);
  assert.equal(task.selection.specifications.requested.length, 0);
  assert.equal(task.selection.specifications.expected.length, expectedSpecifications.length);
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

test('reference qualification records whether its controller is the supported Linux appliance', () => {
  assert.deepEqual(referenceQualificationRunner({ env: { STACK_BENCH_APPLIANCE: '1' },
    platform: 'linux', architecture: 'x64', dockerInfo: {
      ServerVersion: '29.1.2', OSType: 'linux', Architecture: 'x86_64',
      KernelVersion: '6.8.0-test', NCPU: 8, MemTotal: 16_000_000_000,
    } }), {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
    dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
    kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000,
  });
  assert.deepEqual(referenceQualificationRunner({ env: {}, platform: 'win32', architecture: 'x64' }), {
    schemaVersion: 1, mode: 'local-controller', platform: 'win32', architecture: 'x64',
  });
  assert.throws(() => referenceQualificationRunner({ env: { STACK_BENCH_APPLIANCE: '1' },
    platform: 'linux', architecture: 'x64', dockerInfo: {} }),
  /Docker daemon inspection did not return ServerVersion/);
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

test('bounded execution tees useful tails and caps durable process logs', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-process-log-'));
  try {
    const result = await runBounded(process.execPath,
      ['-e', 'process.stdout.write("abcdefgh"); process.stderr.write("actual failure\\n")'], {
        stdio: 'inherit', timeoutMs: 5_000,
        logs: { stdout: join(root, 'stdout.log'), stderr: join(root, 'stderr.log'), maxBytes: 5 },
      });
    assert.equal(result.ok, true);
    assert.equal(readFileSync(join(root, 'stdout.log'), 'utf8'), 'abcde');
    assert.equal(readFileSync(join(root, 'stderr.log'), 'utf8'), 'actua');
    assert.deepEqual({ bytes: result.logs.stdout.bytes, retainedBytes: result.logs.stdout.retainedBytes,
      truncated: result.logs.stdout.truncated }, { bytes: 8, retainedBytes: 5, truncated: true });
    assert.match(result.stderrTail, /actual failure/);
    assert.equal(result.logs.stdout.sha256, createHash('sha256').update('abcde').digest('hex'));
  } finally { rmSync(root, { recursive: true, force: true }); }
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
