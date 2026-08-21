import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

import { createArtifact, currentEngineIdentity, recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.mjs';
import { calibrationQualificationIdentity, resolveCalibrationForRelease } from '../src/composition/calibration-compiler.mjs';
import { PACK_RUNTIME_METRIC } from '../src/composition/pack-runtime.mjs';
import { loadPackBudgetEvidence, PACK_BUDGET_POLICY, parsePackBudgetArgs,
  recommendPackBudgets } from '../src/composition/pack-budget.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

const track = loadTrack('ecommerce');
const binding = resolveRecipeRelease(track, 1);
const calibration = structuredClone(resolveCalibrationForRelease(binding.release, { trackRoot: track.dir }));
calibration.qualification.runner = {
  schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
};
const applianceRunner = Object.freeze({ ...calibration.qualification.runner,
  dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
  kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000 });

function runtime(stackIndex, repetition) {
  const counts = new Map(binding.release.checkCatalog.map(check => [check.packId, 0]));
  for (const check of binding.release.checkCatalog) counts.set(check.packId, counts.get(check.packId) + 1);
  return { schemaVersion: 1, metric: PACK_RUNTIME_METRIC,
    packs: [...counts].sort(([a], [b]) => a.localeCompare(b)).map(([id, checkCount], packIndex) => {
      const measuredRuntimeMs = 1_000 + stackIndex * 100 + repetition * 10 + packIndex;
      return { id, checkCount, setupRuntimeMs: 100, criterionRuntimeMs: measuredRuntimeMs - 100,
        measuredRuntimeMs, budget: { status: 'unmeasured' }, exceeded: null };
    }) };
}

function reference(stack, stackIndex, overrides = {}) {
  const identity = calibrationQualificationIdentity(calibration);
  const fixture = calibration.references.entries.find(entry => entry.backend === stack);
  return createArtifact({ kind: 'reference_qualification', id: `reference-${stack}`,
    identities: recipeArtifactIdentities(binding.release, {
      engine: currentEngineIdentity(), calibration: identity, stackAdapter: { id: stack },
      fixture: { id: fixture.id, sha256: fixture.sourceSha256 },
    }),
    payload: { fixture: fixture.id, fixtureSha256: fixture.sourceSha256,
      requiredRepetitions: 1, isolation: 'docker', mutationControl: false,
      runner: { ...applianceRunner },
      stable: true, sameImage: true, sameHarness: true, harnessSha256: 'b'.repeat(64), ok: true,
      runs: [1].map(repetition => ({ repetition, ok: true, packRuntime: runtime(stackIndex, repetition) })),
      ...overrides } });
}

function exactEvidence() {
  return ['mongodb', 'postgres', 'spacetime'].map((stack, index) => ({
    path: `${stack}.json`, sha256: String(index).repeat(64), artifact: reference(stack, index),
    runtimeCalibration: { id: calibration.id, version: calibration.version,
      sha256: calibration.contentSha256 },
  }));
}

test('budget recommendation requires every exact reference repetition and applies the published rule', () => {
  const result = recommendPackBudgets({ binding, calibration, evidence: exactEvidence() });
  assert.equal(result.samples.length, binding.plan.packs.length * 3);
  assert.equal(result.recommendations.length, binding.plan.packs.length);
  assert(result.recommendations.every(item => item.sampleCount === 3));
  assert(result.recommendations.every(item => item.maxRuntimeMs === 3_000));
  assert.equal(PACK_BUDGET_POLICY.multiplier, 2);
  assert.deepEqual(result.measuredEngine, currentEngineIdentity());
  assert.deepEqual(result.measuredRunner, applianceRunner);
});

test('budget recommendation rejects mutation, duplicate, incomplete, and cross-scope evidence', () => {
  const mutation = exactEvidence();
  mutation[0].artifact.payload.mutationControl = true;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: mutation }), /mutation evidence/);
  const duplicate = exactEvidence();
  duplicate[2].artifact.identities.stackAdapter.id = 'postgres';
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: duplicate }), /repeats stack/);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: exactEvidence().slice(1) }),
    /cover each supported stack/);
  const stale = exactEvidence();
  stale[0].artifact.identities.recipe.sha256 = 'f'.repeat(64);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: stale }),
    /does not match the selected qualification scope/);
  const staleRuntime = exactEvidence();
  staleRuntime[0].runtimeCalibration.sha256 = 'f'.repeat(64);
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: staleRuntime }),
    /retainedRuntimeCalibration.sha256/);
});

test('budget recommendation rejects timing captured outside the Linux appliance', () => {
  const local = exactEvidence();
  local[0].artifact.payload.runner.mode = 'local-controller';
  local[0].artifact.payload.runner.platform = 'win32';
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: local }),
    /not supported appliance timing evidence/);

  const legacy = exactEvidence();
  delete legacy[0].artifact.payload.runner;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: legacy }),
    /runner\.schemaVersion must be 1/);

  const unobserved = exactEvidence();
  unobserved[0].artifact.payload.runner = { ...calibration.qualification.runner };
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: unobserved }),
    /runner observation is missing/);

  const mixed = exactEvidence();
  mixed[1].artifact.payload.runner.cpuCount = 16;
  assert.throws(() => recommendPackBudgets({ binding, calibration, evidence: mixed }),
    /different appliance runner environment/);
});

test('budget CLI parsing requires explicit unique evidence and output', () => {
  const parsed = parsePackBudgetArgs(['node', 'pack-budget.mjs', 'recommend', '--track', 'ecommerce',
    '--level', '1', '--evidence', 'mongo.json', '--out', 'budgets.json']);
  assert.equal(parsed.command, 'recommend');
  assert.equal(parsed.evidence.length, 1);
  assert.equal(parsePackBudgetArgs(['node', 'pack-budget.mjs', 'recommend', '--track', 'ecommerce',
    '--level', '1', '--recipe', 'ecommerce.l1-standard@1.1.0',
    '--evidence', 'mongo.json', '--out', 'budgets.json']).recipe,
  'ecommerce.l1-standard@1.1.0');
  assert.throws(() => parsePackBudgetArgs(['node', 'pack-budget.mjs', 'recommend',
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
          calibration: { id: calibration.id, version: calibration.version,
            sha256: calibration.contentSha256 } }),
        payload: { packRuntime: run.packRuntime } });
    }
    writeArtifact(path, artifact);
    const loaded = loadPackBudgetEvidence([path]);
    assert.equal(loaded.length, 1);
    assert.match(loaded[0].sha256, /^[a-f0-9]{64}$/);
    assert.equal(loaded[0].runtimeCalibration.sha256, calibration.contentSha256);

    const bundlePath = join(root, 'mongodb.runs', 'r1', 'grading', 'bundle.json');
    const changed = createArtifact({ kind: 'grade_bundle', id: 'changed',
      identities: recipeArtifactIdentities(binding.release, {
        engine: artifact.identities.engine, stackAdapter: artifact.identities.stackAdapter,
        calibration: { id: calibration.id, version: calibration.version,
          sha256: calibration.contentSha256 } }),
      payload: { packRuntime: runtime(2, 2) } });
    writeArtifact(bundlePath, changed);
    assert.throws(() => loadPackBudgetEvidence([path]), /summary differs/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
