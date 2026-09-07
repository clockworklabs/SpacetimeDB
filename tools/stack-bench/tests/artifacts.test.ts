import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ARTIFACT_SCHEMA_VERSION, createArtifact, readArtifact, readArtifactPayload,
  readRunJson, writeArtifact, writeRunJson } from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';

test('run artifacts are atomic and identify the producing run', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-artifact-'));
  try {
    const path = join(root, 'run.json');
    const mode = { id: 'sequential', version: '1.0.0' };
    const featureCatalog = { id: 'catalog', sha256: 'a'.repeat(64) };
    writeRunJson(path, { id: 'run-a', mode, featureCatalog, levels: [] });
    const artifact = readRunJson(path, 'run-a');
    assert.equal(artifact.id, 'run-a');
    assert.deepEqual(artifact.mode, mode);
    assert.deepEqual(artifact.featureCatalog, featureCatalog);
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
        recipe: { id: 'ecommerce-l1', sha256: digest },
        fixture: { id: 'standard', sha256: digest },
        calibration: { id: 'l1-standard', sha256: digest },
        packs: [{ id: 'auth', sha256: digest }],
        stackAdapter: { id: 'postgres', sha256: digest },
      },
      payload: { total: 1, max: 1, features: [] },
    });
    const artifact = readArtifact(path, { expectedKind: 'grade', expectedId: 'grade-child' });
    assert.equal(artifact.attempt.parentId, 'bundle-parent');
    assert.equal(artifact.identities.recipe?.sha256, digest);
    assert.equal(artifact.identities.packs[0]?.sha256, digest);
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

test('campaign extension provenance is strict and records completed rechecks', () => {
  const seed = { fromDepth: 2, sourceSha256: 'a'.repeat(64), sourceFiles: 4,
    parent: { campaignId: 'parent', campaignSha256: 'b'.repeat(64),
      attemptId: 'attempt-1', executionId: 'execution-1', runId: 'run-1',
      runSha256: 'c'.repeat(64) }, validatedDepths: [1, 2] };
  assert.doesNotThrow(() => createArtifact({ kind: 'benchmark_run', id: 'extension',
    payload: { progressionSeed: seed } }));
  assert.throws(() => createArtifact({ kind: 'benchmark_run', id: 'bad-extension',
    payload: { progressionSeed: { ...seed, validatedDepths: [2, 1] } } }),
  /validatedDepths is invalid/);
});

test('unknown kinds, fields, malformed payloads, and backward timestamps fail closed', () => {
  assert.throws(() => createArtifact({ kind: 'mystery', id: 'x' }), /unknown kind/);
  assert.throws(() => createArtifact({ kind: 'benchmark_run', id: 'x',
    payload: { outcome: { kind: 'typo' } } }), /outcome\.kind is invalid/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x', payload: { features: {} } }),
    /payload\.features must be an array/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x',
    timestamps: { startedAt: '2026-08-12T12:00:01.000Z', completedAt: '2026-08-12T12:00:00.000Z' } }),
  /precedes/);
  const valid = createArtifact({ kind: 'grade', id: 'x',
    payload: { total: 0, max: 0, features: [] } });
  assert.throws(() => writeArtifact('unused.json', { ...valid, surprise: true }), /surprise is unknown/);
  const missingIdentities: Partial<typeof valid> = structuredClone(valid);
  delete missingIdentities.identities;
  assert.throws(() => writeArtifact('unused.json', missingIdentities), /identities is required/);
  const incompleteTimestamps: Omit<typeof valid, 'timestamps'> & {
    timestamps: Partial<typeof valid.timestamps>;
  } = structuredClone(valid);
  delete incompleteTimestamps.timestamps.completedAt;
  assert.throws(() => writeArtifact('unused.json', incompleteTimestamps), /timestamps is incomplete/);
  const missingIdentitySlot: Omit<typeof valid, 'identities'> & {
    identities: Partial<typeof valid.identities>;
  } = structuredClone(valid);
  delete missingIdentitySlot.identities.experiment;
  assert.throws(() => writeArtifact('unused.json', missingIdentitySlot), /identities\.experiment is required/);
  const incompleteEngineIdentity = structuredClone(valid);
  const engine = incompleteEngineIdentity.identities.engine;
  assert(engine);
  delete (engine as Partial<typeof engine>).sha256;
  assert.throws(() => writeArtifact('unused.json', incompleteEngineIdentity),
    /identities\.engine\.sha256 is required/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'x', identities: { engine: null } }),
    /identities\.engine is required/);
  const retiredIdentityFields = { id: 'old', sha256: 'a'.repeat(64), version: '1.0.0' };
  for (const identities of [
    { recipe: retiredIdentityFields },
    { fixture: retiredIdentityFields },
    { calibration: retiredIdentityFields },
    { packs: [retiredIdentityFields] },
  ]) {
    assert.throws(() => createArtifact({ kind: 'grade', id: 'x', identities }),
      /version is unknown/);
  }
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
  assert.doesNotThrow(() => createArtifact({ kind: 'grade_bundle', id: 'source-bound-scored',
    payload: { observation: 'scored', source: { sha256: 'b'.repeat(64) }, suites: {}, totals: {} } }));
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'incomplete-reference' }),
    /payload\.fixture is required/);
  const referencePayload = { fixture: 'standard', requiredRepetitions: 1, runs: [] };
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'bad-runner', payload: {
    ...referencePayload,
    runner: { schemaVersion: 1, mode: 'desktop', platform: 'win32', architecture: 'x64' },
  } }), /runner\.mode is invalid/);
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'future-runner', payload: {
    ...referencePayload,
    runner: { schemaVersion: 2, mode: 'appliance', platform: 'linux', architecture: 'x64' },
  } }), /runner\.schemaVersion must be 1/);
  assert.doesNotThrow(() => createArtifact({ kind: 'null_control', id: 'typed-runner', payload: {
    runner: { schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
      dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
      kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000 },
  } }));
  assert.doesNotThrow(() => createArtifact({ kind: 'reference_qualification', id: 'base-runner', payload: {
    ...referencePayload,
    runner: { schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64' },
  } }));
  assert.throws(() => createArtifact({ kind: 'reference_qualification', id: 'partial-runner', payload: {
    ...referencePayload,
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
  } }), /unique subset of the assigned mutation IDs/);
  assert.doesNotThrow(() => createArtifact({ kind: 'mutation_control', id: 'partial-shard', payload: {
    shard: { index: 1, count: 3, mutationIds: ['broken-auth', 'broken-owner'] },
    results: [results[0]], checkpoint: { status: 'incomplete' },
  } }));
  assert.throws(() => createArtifact({ kind: 'mutation_control', id: 'partial-marked-complete', payload: {
    shard: { index: 1, count: 3, mutationIds: ['broken-auth', 'broken-owner'] },
    results: [results[0]], checkpoint: { status: 'complete' },
  } }), /complete mutation_control.*match the exact result set/);
  assert.throws(() => createArtifact({ kind: 'mutation_control', id: 'duplicate-partial-result', payload: {
    shard: { index: 1, count: 3, mutationIds: ['broken-auth', 'broken-owner'] },
    results: [results[0], results[0]], checkpoint: { status: 'incomplete' },
  } }), /unique subset of the assigned mutation IDs/);
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
    assert.throws(() => writeArtifact(path, { kind: 'benchmark_run', id: 'secret-run',
      payload: { backendLease: { api_key: 'do-not-persist' } } }), /secret-bearing/);
    assert.equal(existsSync(path), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('typed grade evidence is required and obsolete projection fields are rejected', () => {
  const evidence = createCheckEvidence({ status: 'failed', code: 'application_failure', phase: 'assertion',
    summary: 'not observed', startedAtMs: 1, completedAtMs: 2 });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const criterion = { id: 'works', desc: 'works', points: 1, evidence };
  assert.doesNotThrow(() => createArtifact({ kind: 'grade', id: 'typed-grade',
    payload: { total: 0, max: 1,
      features: [{ id: 1, name: 'feature', score: 0, max: 1,
        setupEvidence, criteria: [criterion] }] } }));
  assert.throws(() => createArtifact({ kind: 'grade', id: 'obsolete-grade',
    payload: { total: 0, max: 1, features: [{ id: 1, name: 'feature', score: 0,
      max: 1, setupEvidence,
      criteria: [{ ...criterion, passed: false }] }] } }), /passed is obsolete/);
  assert.throws(() => createArtifact({ kind: 'grade', id: 'missing-evidence',
    payload: { total: 0, max: 1,
      features: [{ id: 1, name: 'feature', score: 0, max: 1,
        setupEvidence, criteria: [{ id: 'works' }] }] } }),
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
