import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { ARTIFACT_SCHEMA_VERSION, createArtifact, readArtifact, readArtifactPayload,
  readRunJson, writeArtifact, writeRunJson } from '../src/evidence/artifacts.mjs';
import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';

const BENCH_ROOT = join(import.meta.dirname, '..');

function freshEngineIdentity(root = BENCH_ROOT) {
  const artifactsUrl = pathToFileURL(join(root, 'src', 'evidence', 'artifacts.mjs')).href;
  const result = spawnSync(process.execPath, ['--input-type=module', '--eval',
    `import { currentEngineIdentity } from ${JSON.stringify(artifactsUrl)}; console.log(JSON.stringify(currentEngineIdentity()));`],
  { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

function writableEngineRoot() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-engine-'));
  const root = join(temp, 'stack-bench');
  const excluded = new Set(['archive', 'node_modules', 'reference-apps', 'results', 'tests', 'tracks']);
  cpSync(BENCH_ROOT, root, { recursive: true, filter: source => {
    if (source === BENCH_ROOT) return true;
    return !excluded.has(relative(BENCH_ROOT, source).split(/[\\/]/)[0]);
  } });
  return { temp, root };
}

test('run artifacts are atomic and identify the producing run', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    writeRunJson(path, { id: 'run-a', levels: [] });
    const artifact = readRunJson(path, 'run-a');
    assert.equal(artifact.id, 'run-a');
    assert.equal(artifact.artifactSchemaVersion, ARTIFACT_SCHEMA_VERSION);
    assert.throws(() => readRunJson(path, 'run-b'), /belongs to run-a/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an invalid replacement cannot overwrite an existing run artifact', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    writeRunJson(path, { id: 'run-a', status: 'complete' });
    assert.throws(() => writeRunJson(path, { id: '', complete: false }), /non-empty id/);
    assert.equal(readRunJson(path, 'run-a').status, 'complete');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an unversioned stale file is rejected before its run id can be trusted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    writeFileSync(path, JSON.stringify({ id: 'old-run' }));
    assert.throws(() => readRunJson(path, 'current-run'), /unsupported schema missing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('historical unversioned artifacts are not accepted by the active reader', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    writeFileSync(path, JSON.stringify({ id: 'legacy-run', complete: true }));
    assert.throws(() => readRunJson(path, 'legacy-run'), /unsupported schema missing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('unknown artifact schemas fail rather than being guessed', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    writeFileSync(path, JSON.stringify({ artifactSchemaVersion: 999, id: 'future-run' }));
    assert.throws(() => readRunJson(path, 'future-run'), /unsupported schema 999/);
    assert.throws(() => writeArtifact(path, { artifactSchemaVersion: 1,
      kind: 'benchmark_run', id: 'old-envelope' }), /unsupported schema 1/);
    assert.throws(() => writeRunJson(path, { artifactSchemaVersion: 999, id: 'future-run' }),
      /schema 999 is not supported/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('v2 envelopes preserve exact identities, timestamps, and attempt ancestry', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'grade.json');
    const digest = 'a'.repeat(64);
    writeArtifact(path, {
      kind: 'grade', id: 'grade-child',
      attempt: { id: 'grade-child', parentId: 'bundle-parent' },
      timestamps: { startedAt: '2026-08-12T12:00:00.000Z', completedAt: '2026-08-12T12:00:01.000Z' },
      identities: {
        recipe: { id: 'ecommerce-l1', version: '1.0.0', sha256: digest, state: 'draft' },
        fixture: { id: 'standard', version: '1.0.0', sha256: digest, state: 'draft' },
        calibration: { id: 'l1-standard', version: '1.0.0', sha256: digest, state: 'draft' },
        packs: [{ id: 'auth', version: '1.0.0', sha256: digest, state: 'draft' }],
        stackAdapter: { id: 'postgres', sha256: digest },
      },
      payload: { total: 1, max: 1, features: [] },
    });
    const artifact = readArtifact(path, { expectedKind: 'grade', expectedId: 'grade-child' });
    assert.equal(artifact.attempt.parentId, 'bundle-parent');
    assert.equal(artifact.identities.recipe.sha256, digest);
    assert.equal(artifact.identities.packs[0].sha256, digest);
    assert.equal(artifact.timestamps.completedAt, '2026-08-12T12:00:01.000Z');
    assert.deepEqual(readArtifactPayload(path).features, []);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('partial and invalid attempts are representable without invented scores', () => {
  const artifact = createArtifact({
    kind: 'benchmark_run', id: 'partial-run',
    attempt: { id: 'partial-run', parentId: null },
    timestamps: { startedAt: '2026-08-12T12:00:00.000Z', completedAt: null },
    payload: { status: 'invalid', outcome: { kind: 'harness_failure', reason: 'agent exited' } },
  });
  assert.equal(artifact.timestamps.completedAt, null);
  assert.equal(artifact.payload.status, 'invalid');
  assert.equal('score' in artifact.payload, false);
  assert.equal('totals' in artifact.payload, false);
});

test('unknown kinds, fields, malformed payloads, and backward timestamps fail closed', () => {
  assert.throws(() => createArtifact({ kind: 'mystery', id: 'x' }), /unknown kind/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x', payload: { features: {} } }),
    /payload\.features must be an array/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x',
    timestamps: { startedAt: '2026-08-12T12:00:01.000Z', completedAt: '2026-08-12T12:00:00.000Z' } }),
  /precedes/);
  const valid = createArtifact({ kind: 'grade', id: 'x' });
  assert.throws(() => writeArtifact('unused.json', { ...valid, surprise: true }), /surprise is unknown/);
  const missingIdentities = structuredClone(valid);
  delete missingIdentities.identities;
  assert.throws(() => writeArtifact('unused.json', missingIdentities), /identities is required/);
  const incompleteTimestamps = structuredClone(valid);
  delete incompleteTimestamps.timestamps.completedAt;
  assert.throws(() => writeArtifact('unused.json', incompleteTimestamps), /timestamps is incomplete/);
  const missingIdentitySlot = structuredClone(valid);
  delete missingIdentitySlot.identities.experiment;
  assert.throws(() => writeArtifact('unused.json', missingIdentitySlot), /identities\.experiment is required/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x', identities: { engine: null } }),
    /identities\.engine is required/);
  const observedPayload = { observation: 'observed', source: { sha256: 'a'.repeat(64) },
    suites: {}, totals: {}, selection: { observation: 'observed', scoredPoints: 0,
      observedPoints: 1 } };
  assert.doesNotThrow(() => createArtifact({ kind: 'grade_bundle', id: 'observed',
    payload: observedPayload }));
  assert.throws(() => createArtifact({ kind: 'grade_bundle', id: 'unbound-observed', payload: {
    ...observedPayload, source: undefined,
  } }), /first-build SHA-256/);
  assert.throws(() => createArtifact({ kind: 'grade_bundle', id: 'scored-observed', payload: {
    ...observedPayload, selection: { ...observedPayload.selection, scoredPoints: 1 },
  } }), /contribute zero score/);
  assert.doesNotThrow(() => createArtifact({ kind: 'reference_qualification', id: 'legacy-reference' }));
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'bad-runner', payload: {
    runner: { schemaVersion: 1, mode: 'desktop', platform: 'win32', architecture: 'x64' },
  } }), /runner\.mode is invalid/);
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'future-runner', payload: {
    runner: { schemaVersion: 2, mode: 'appliance', platform: 'linux', architecture: 'x64' },
  } }), /runner\.schemaVersion must be 1/);
  assert.doesNotThrow(() => createArtifact({ kind: 'null_control', id: 'typed-runner', payload: {
    runner: { schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
      dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
      kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000 },
  } }));
  assert.doesNotThrow(() => createArtifact({ kind: 'reference_qualification', id: 'legacy-base-runner', payload: {
    runner: { schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64' },
  } }));
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'partial-runner', payload: {
    runner: { schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
      dockerEngineVersion: '29.1.2' },
  } }), /runner\.dockerOs must be a non-empty string/);
});

test('mutation shard artifacts bind exact coordinates to the exact result set', () => {
  const results = [{ id: 'broken-auth', status: 'CAUGHT' },
    { id: 'broken-owner', status: 'CAUGHT' }];
  assert.doesNotThrow(() => createArtifact({ kind: 'mutation_control', id: 'shard', payload: {
    shard: { index: 1, count: 3, mutationIds: ['broken-owner', 'broken-auth'] }, results,
  } }));
  assert.throws(() => createArtifact({ kind: 'mutation_control', id: 'bad-index', payload: {
    shard: { index: 3, count: 3, mutationIds: ['broken-auth'] }, results: [results[0]],
  } }), /coordinates are invalid/);
  assert.throws(() => createArtifact({ kind: 'mutation_control', id: 'wrong-result', payload: {
    shard: { index: 1, count: 3, mutationIds: ['other'] }, results: [results[0]],
  } }), /match the exact result set/);
  assert.throws(() => createArtifact({ kind: 'mutation_control', id: 'unknown-shard-field', payload: {
    shard: { index: 1, count: 3, mutationIds: ['broken-auth'], worker: true }, results: [results[0]],
  } }), /worker is unknown/);
});

test('public artifacts reject secret-bearing fields before touching disk', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    assert.throws(() => writeArtifact(path, { kind: 'benchmark_run', id: 'secret-run',
      payload: { backendLease: { leaseToken: 'do-not-persist' } } }), /secret-bearing/);
    assert.equal(existsSync(path), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('typed grade evidence is required and obsolete projection fields are rejected', () => {
  const evidence = createCheckEvidence({ status: 'failed', code: 'application_failure', phase: 'assertion',
    summary: 'not observed', startedAtMs: 1, completedAtMs: 2 });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const criterion = { id: 'works', points: 1, evidence };
  assert.doesNotThrow(() => createArtifact({ kind: 'grade', id: 'typed-grade',
    payload: { features: [{ id: 1, setupEvidence, criteria: [criterion] }] } }));
  assert.throws(() => createArtifact({ kind: 'grade', id: 'obsolete-grade',
    payload: { features: [{ id: 1, setupEvidence,
      criteria: [{ ...criterion, passed: false }] }] } }), /passed is obsolete/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'missing-evidence',
    payload: { features: [{ id: 1, setupEvidence, criteria: [{ id: 'works' }] }] } }),
  /evidence is required/);
});

test('active readers reject unversioned files without inferring their kind', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const bundle = join(root, 'bundle.json');
    writeFileSync(bundle, JSON.stringify({ suites: {}, totals: {} }));
    assert.throws(() => readArtifact(bundle, { expectedKind: 'grade_bundle' }),
      /unsupported schema missing/);
    const unknown = join(root, 'unknown.json');
    writeFileSync(unknown, JSON.stringify({ kind: 'future_thing', id: 'x' }));
    assert.throws(() => readArtifact(unknown), /unsupported schema missing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('engine identity ignores hidden runtime work directories', () => {
  const copy = writableEngineRoot();
  const before = freshEngineIdentity(copy.root);
  const runtime = mkdtempSync(join(copy.root, '.engine-runtime-'));
  try {
    writeFileSync(join(runtime, 'generated.mjs'), 'this is generated runtime state, not harness code\n');
    writeFileSync(join(runtime, 'session.json'), '{"turns":999}\n');
    assert.deepEqual(freshEngineIdentity(copy.root), before);
  } finally { rmSync(copy.temp, { recursive: true, force: true }); }
});

test('engine identity excludes the inert historical archive', () => {
  const copy = writableEngineRoot();
  const before = freshEngineIdentity(copy.root);
  mkdirSync(join(copy.root, 'archive'));
  const runtime = mkdtempSync(join(copy.root, 'archive', '.engine-runtime-'));
  try {
    writeFileSync(join(runtime, 'historical.mjs'), 'archived generated code is not the active harness\n');
    assert.deepEqual(freshEngineIdentity(copy.root), before);
  } finally { rmSync(copy.temp, { recursive: true, force: true }); }
});

test('engine identity excludes the generated controller dependency manifest', () => {
  const copy = writableEngineRoot();
  const before = freshEngineIdentity(copy.root);
  try {
    writeFileSync(join(copy.root, 'dependency-manifest.json'),
      JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString() }));
    assert.deepEqual(freshEngineIdentity(copy.root), before);
  } finally { rmSync(copy.temp, { recursive: true, force: true }); }
});

test('engine identity excludes installed dependencies at any directory depth', () => {
  const copy = writableEngineRoot();
  const before = freshEngineIdentity(copy.root);
  try {
    const nestedDependencies = join(copy.root, 'grader', 'node_modules', 'generated-package');
    mkdirSync(nestedDependencies, { recursive: true });
    writeFileSync(join(nestedDependencies, 'index.js'), 'installed dependency, not harness source\n');
    writeFileSync(join(nestedDependencies, 'package.json'), '{"name":"generated-package"}\n');
    assert.deepEqual(freshEngineIdentity(copy.root), before);
  } finally { rmSync(copy.temp, { recursive: true, force: true }); }
});
