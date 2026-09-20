import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

import { createArtifact, currentEngineIdentity, recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { calibrationQualificationIdentity, calibrationQualificationRelease,
  resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { PACK_RUNTIME_METRIC } from '../src/composition/pack-runtime.js';
import { parsePackBudgetArgs } from '../commands/pack-budget.js';
import { loadPackBudgetEvidence, PACK_BUDGET_POLICY, recommendPackBudgets }
  from '../src/composition/pack-budget.js';
import type { PackBudgetEvidence, PackRuntime, ReferenceQualificationPayload,
  RunnerObservation } from '../src/composition/pack-budget.js';
import type { Artifact, ArtifactIdentity } from '../src/evidence/artifacts.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';

const track = loadTrack('ecommerce');
const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
const resolvedCalibration = resolveCalibrationForRelease(binding.release, { trackRoot: track.dir, alias: 'L3' });
assert(resolvedCalibration);
const calibration = structuredClone(resolvedCalibration);
const selectedChecks = calibrationQualificationRelease(calibration,
  binding.release, binding.execution).release.checkCatalog;
calibration.qualification.runner = {
  schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
};
const applianceRunner: RunnerObservation = Object.freeze({ ...calibration.qualification.runner,
  dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
  kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000 });

function runtime(stackIndex: number, repetition: number): PackRuntime {
  const counts = new Map<string, number>(binding.plan.packs.map(pack => [pack.id, 0]));
  for (const check of selectedChecks) {
    if (!check.packId) throw new Error(`check ${check.stableKey} has no pack`);
    counts.set(check.packId, (counts.get(check.packId) ?? 0) + 1);
  }
  return { schemaVersion: 1, metric: PACK_RUNTIME_METRIC,
    packs: [...counts].filter(([, count]) => count > 0)
      .sort(([a], [b]) => a.localeCompare(b)).map(([id, checkCount], packIndex) => {
      const measuredRuntimeMs = 1_000 + stackIndex * 100 + repetition * 10 + packIndex;
      return { id, checkCount, setupRuntimeMs: 100, criterionRuntimeMs: measuredRuntimeMs - 100,
        measuredRuntimeMs, budget: { status: 'unmeasured' }, exceeded: null };
    }) };
}

function reference(stack: string, stackIndex: number,
  overrides: Partial<ReferenceQualificationPayload> = {}): Artifact<ReferenceQualificationPayload> {
  const fixture = calibration.references.entries.find(entry => entry.backend === stack);
  assert(fixture);
  return createArtifact({ kind: 'reference_qualification', id: `reference-${stack}`,
    identities: recipeArtifactIdentities(binding.release, {
      engine: currentEngineIdentity(), calibration: { id: calibration.id,
        sha256: calibrationQualificationIdentity(calibration).contentSha256 }, stackAdapter: { id: stack },
      fixture: { id: fixture.id, sha256: fixture.sourceSha256 },
    }),
    payload: { fixture: fixture.id, fixtureSha256: fixture.sourceSha256,
      requiredRepetitions: 1, isolation: 'docker', mutationControl: false,
      runner: { ...applianceRunner },
      stable: true, sameImage: true, sameHarness: true, harnessSha256: 'b'.repeat(64), ok: true,
      runs: [1].map(repetition => ({ repetition, ok: true, packRuntime: runtime(stackIndex, repetition) })),
      ...overrides } });
}

function exactEvidence(): PackBudgetEvidence[] {
  return calibration.qualification.stacks.map((stack, index) => ({
    path: `${stack}.json`, sha256: String(index).repeat(64), artifact: reference(stack, index),
    runtimeCalibration: { id: calibration.id,
      sha256: calibration.contentSha256 },
  }));
}

function evidenceAt(evidence: PackBudgetEvidence[], index: number): PackBudgetEvidence {
  const item = evidence[index];
  assert(item);
  return item;
}

function identity(value: ArtifactIdentity | null): ArtifactIdentity {
  assert(value);
  return value;
}

test('budget recommendation requires every exact reference repetition and applies the published rule', () => {
  const evidence = exactEvidence();
  evidence.forEach((item, index) => {
    assert(item.artifact.payload.runner);
    item.artifact.payload.runner.containersRunning = 18 + index;
  });
  const original = structuredClone(evidence);
  const result = recommendPackBudgets({ binding, calibration, evidence });
  const measuredPackCount = new Set(selectedChecks.map(check => check.packId)).size;
  assert.equal(result.samples.length, measuredPackCount * evidence.length);
  assert.equal(result.recommendations.length, measuredPackCount);
  assert(result.recommendations.every(item => item.sampleCount === evidence.length));
  assert(result.recommendations.every(item => item.maxRuntimeMs === 3_000));
  assert.equal(PACK_BUDGET_POLICY.multiplier, 2);
  assert.deepEqual(result.measuredEngine, currentEngineIdentity());
  assert.deepEqual(result.measuredRunner, { ...applianceRunner, containersRunning: 18 });
  assert.deepEqual(evidence, original);
  assert.notEqual(evidence[0]!.artifact.identities.calibration!.sha256, evidence[0]!.runtimeCalibration!.sha256,
    'qualification and runtime calibration hashes have different meanings');
});

test('budget recommendation rejects mutation, duplicate, incomplete, and cross-scope evidence', () => {
  const diagnostic = exactEvidence();
  evidenceAt(diagnostic, 0).artifact.payload.diagnostic = true;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: diagnostic }), /targeted diagnostic/);
  evidenceAt(diagnostic, 0).artifact.payload.timingOnly = true;
  assert.doesNotThrow(() => recommendPackBudgets({ binding, calibration, evidence: diagnostic }));
  const mutation = exactEvidence();
  evidenceAt(mutation, 0).artifact.payload.mutationControl = true;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: mutation }), /mutation evidence/);
  const duplicate = exactEvidence();
  identity(evidenceAt(duplicate, 2).artifact.identities.stackAdapter).id = identity(evidenceAt(duplicate, 0).artifact.identities.stackAdapter).id;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: duplicate }), /repeats stack/);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: exactEvidence().slice(1) }),
    /cover each supported stack/);
  const stale = exactEvidence();
  identity(evidenceAt(stale, 0).artifact.identities.recipe).sha256 = 'f'.repeat(64);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: stale }),
    /does not match the selected qualification scope/);
  const staleRuntime = exactEvidence();
  const staleRuntimeIdentity = evidenceAt(staleRuntime, 0).runtimeCalibration;
  assert(staleRuntimeIdentity);
  staleRuntimeIdentity.sha256 = 'f'.repeat(64);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: staleRuntime }),
    /retainedRuntimeCalibration.sha256/);
  const wrongQualification = exactEvidence();
  identity(evidenceAt(wrongQualification, 0).artifact.identities.calibration).sha256 = calibration.contentSha256;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: wrongQualification }),
    /identities.calibration.sha256/);
});

test('progression L3 budgets require exactly the selected pack counts, not the full catalog', () => {
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  const calibration = resolveCalibrationForRelease(binding.release, { trackRoot: track.dir, alias: 'L3' });
  assert(calibration);
  const selected = new Set(calibration.qualification.checks);
  const counts = new Map<string, number>();
  for (const check of binding.release.checkCatalog.filter(check => selected.has(check.stableKey))) {
    assert(check.packId);
    counts.set(check.packId, (counts.get(check.packId) ?? 0) + 1);
  }
  assert(selected.size < binding.release.checkCatalog.length);
  const evidence = exactEvidence();
  for (const item of evidence) {
    const fixture = calibration.references.entries.find(entry => entry.backend === item.artifact.identities.stackAdapter?.id);
    assert(fixture);
    item.runtimeCalibration = { id: calibration.id, sha256: calibration.contentSha256 };
    item.artifact.identities = recipeArtifactIdentities(binding.release, {
      ...item.artifact.identities, recipe: { id: binding.release.id, sha256: binding.release.contentSha256 },
      calibration: { id: calibration.id, sha256: calibrationQualificationIdentity(calibration).contentSha256 },
      fixture: { id: fixture.id, sha256: fixture.sourceSha256 },
    });
    item.artifact.payload.fixture = fixture.id;
    item.artifact.payload.fixtureSha256 = fixture.sourceSha256;
    for (const run of item.artifact.payload.runs) run.packRuntime.packs = [...counts].map(([id, checkCount]) => ({
      id, checkCount, setupRuntimeMs: 100, criterionRuntimeMs: 900, measuredRuntimeMs: 1_000,
    }));
  }
  assert.equal(recommendPackBudgets({ binding, calibration, evidence }).recommendations.length, counts.size);
  const missing = structuredClone(evidence);
  evidenceAt(missing, 0).artifact.payload.runs[0]!.packRuntime.packs.pop();
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: missing }), /missing packs/);
  const extra = structuredClone(evidence);
  const unselected = binding.plan.packs.find(pack => !counts.has(pack.id));
  assert(unselected);
  evidenceAt(extra, 0).artifact.payload.runs[0]!.packRuntime.packs.push({ id: unselected.id,
    checkCount: 1, setupRuntimeMs: 100, criterionRuntimeMs: 900, measuredRuntimeMs: 1_000 });
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: extra }), /unknown pack/);
  const wrongCount = structuredClone(evidence);
  evidenceAt(wrongCount, 0).artifact.payload.runs[0]!.packRuntime.packs[0]!.checkCount = 0;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: wrongCount }), /checks for .* expected/);
});

test('budget recommendation rejects timing captured outside the Linux appliance', () => {
  const local = exactEvidence();
  const localRunner = evidenceAt(local, 0).artifact.payload.runner;
  assert(localRunner);
  localRunner.mode = 'local-controller';
  localRunner.platform = 'win32';
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: local }),
    /not supported appliance timing evidence/);

  const legacy = exactEvidence();
  delete evidenceAt(legacy, 0).artifact.payload.runner;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: legacy }),
    /runner\.schemaVersion must be 1/);

  const unobserved = exactEvidence();
  evidenceAt(unobserved, 0).artifact.payload.runner = { ...calibration.qualification.runner };
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: unobserved }),
    /runner observation is missing/);

  for (const [field, value] of Object.entries({ cpuCount: 16, memoryBytes: 32_000_000_000,
    dockerEngineVersion: '30.0.0', kernelVersion: 'other-kernel', hostname: 'other-host',
    packageRegistry: 'https://other-registry.example/', futureIdentityField: 'other-setting' })) {
    const mixed = exactEvidence();
    const mixedRunner = evidenceAt(mixed, 1).artifact.payload.runner;
    assert(mixedRunner);
    mixedRunner[field] = value;
    assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: mixed }),
      /different appliance runner environment/);
  }
});

test('budget CLI parsing requires explicit unique evidence and output', () => {
  const parsed = parsePackBudgetArgs(['node', 'pack-budget.js', 'recommend', '--track', 'ecommerce',
    '--level', '1', '--evidence', 'mongo.json', '--out', 'budgets.json']);
  assert.equal(parsed.command, 'recommend');
  assert.equal(parsed.evidence.length, 1);
  assert.equal(parsePackBudgetArgs(['node', 'pack-budget.js', 'recommend', '--track', 'ecommerce',
    '--level', '1', '--recipe', 'ecommerce.sequential-l1',
    '--evidence', 'mongo.json', '--out', 'budgets.json']).recipe,
  'ecommerce.sequential-l1');
  assert.throws(() => parsePackBudgetArgs(['node', 'pack-budget.js', 'recommend',
    '--track', 'ecommerce', '--level', '1']), /usage/);
});

test('budget evidence loader verifies retained raw runs against their summary', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-pack-budget-'));
  try {
    const path = join(root, 'mongodb.json');
    const artifact = reference('mongodb', 0);
    for (const run of artifact.payload.runs) {
      run.output = `mongodb.runs/r${run.repetition}`;
      const output = join(root, 'mongodb.runs', `r${run.repetition}`);
      mkdirSync(join(output, 'grading'), { recursive: true });
      const raw = createArtifact({ kind: 'benchmark_run', id: `run-${run.repetition}`,
        identities: recipeArtifactIdentities(null, { engine: artifact.identities.engine,
          agentAdapter: { id: 'reference-fixture' }, stackAdapter: { id: 'mongodb' } }) });
      writeArtifact(join(output, 'run.json'), raw);
      writeArtifact(join(output, 'grading', 'bundle.json'), { kind: 'grade_bundle',
        id: `bundle-${run.repetition}`, identities: recipeArtifactIdentities(binding.release, {
          engine: artifact.identities.engine, stackAdapter: artifact.identities.stackAdapter,
          calibration: { id: calibration.id,
            sha256: calibration.contentSha256 } }),
        payload: { packRuntime: run.packRuntime } });
    }
    writeArtifact(path, artifact);
    const loaded = loadPackBudgetEvidence([path]);
    assert.equal(loaded.length, 1);
    const loadedEvidence = evidenceAt(loaded, 0);
    assert.match(loadedEvidence.sha256, /^[a-f0-9]{64}$/);
    assert(loadedEvidence.runtimeCalibration);
    assert.equal(loadedEvidence.runtimeCalibration.sha256, calibration.contentSha256);

    const bundlePath = join(root, 'mongodb.runs', 'r1', 'grading', 'bundle.json');
    const changed = createArtifact({ kind: 'grade_bundle', id: 'changed',
      identities: recipeArtifactIdentities(binding.release, {
        engine: artifact.identities.engine, stackAdapter: artifact.identities.stackAdapter,
        calibration: { id: calibration.id,
          sha256: calibration.contentSha256 } }),
      payload: { packRuntime: runtime(2, 2) } });
    writeArtifact(bundlePath, changed);
    assert.throws(() => loadPackBudgetEvidence([path]), /summary differs/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
