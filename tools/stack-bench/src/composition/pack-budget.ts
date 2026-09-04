import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, resolve, sep } from 'node:path';

import { ARTIFACT_FILE, currentEngineIdentity, readArtifact } from '../evidence/artifacts.js';
import type { Artifact, ArtifactIdentity }
  from '../evidence/artifacts.js';
import type { CalibrationPlan } from './calibration-compiler.js';
import { canonicalDefinitionJson } from './definition-plan.js';
import { nonNegativeInteger, PACK_RUNTIME_METRIC } from './pack-runtime.js';
import { sha256 } from '../evidence/provenance.js';
import type { RecipeBinding } from './recipe-release.js';
import { missingRunnerObservation } from '../runtime/runner-environment.js';

type UnknownRecord = Record<string, unknown>;

export interface PackRuntimeEntry {
  id: string;
  checkCount: unknown;
  setupRuntimeMs: unknown;
  criterionRuntimeMs: unknown;
  measuredRuntimeMs: unknown;
}

export interface PackRuntime {
  schemaVersion: number;
  metric: string;
  packs: PackRuntimeEntry[];
}

export interface ReferenceRun {
  repetition: number;
  ok: boolean;
  output?: string;
  packRuntime: PackRuntime;
}

export interface RunnerObservation extends UnknownRecord {
  schemaVersion?: number;
  mode?: string;
  platform?: string;
  architecture?: string;
  dockerEngineVersion?: string;
  dockerOs?: string;
  dockerArchitecture?: string;
  kernelVersion?: string;
  cpuCount?: number;
  memoryBytes?: number;
}

export interface ReferenceQualificationPayload {
  fixture: string;
  fixtureSha256: string;
  requiredRepetitions: number;
  isolation: string;
  mutationControl: boolean;
  runner?: RunnerObservation;
  stable: boolean;
  sameImage: boolean;
  sameHarness: boolean;
  ok: boolean;
  runs: ReferenceRun[];
}

export interface PackBudgetEvidence {
  path: string;
  sha256: string;
  artifact: Artifact<ReferenceQualificationPayload>;
  runtimeCalibration: Pick<ArtifactIdentity, 'id' | 'sha256'> | null;
}

export interface PackRuntimeSample {
  stack: string;
  repetition: number;
  packId: string;
  checkCount: number;
  setupRuntimeMs: number;
  criterionRuntimeMs: number;
  measuredRuntimeMs: number;
}

export interface PackBudgetRecommendation {
  packId: string;
  sampleCount: number;
  observedMinRuntimeMs: number;
  observedMaxRuntimeMs: number;
  maxRuntimeMs: number;
}

export interface PackBudgetResult {
  measuredEngine: ArtifactIdentity;
  measuredRunner: RunnerObservation;
  samples: PackRuntimeSample[];
  recommendations: PackBudgetRecommendation[];
}

export const PACK_BUDGET_POLICY = Object.freeze({
  id: 'max-observed-times-two-rounded-up-1s-v1',
  metric: PACK_RUNTIME_METRIC,
  multiplier: 2,
  roundUpMs: 1_000,
  minimumMs: 1_000,
});

function requireSupportedBudgetRunner(runner: RunnerObservation | undefined,
  expectedRunner: object | null | undefined, path: string): void {
  if (!expectedRunner) throw new Error('selected calibration does not declare a qualification runner');
  for (const [field, expected] of Object.entries(expectedRunner)) {
    if (identityField(runner, field) !== expected) {
      throw new Error(`${path} is not supported appliance timing evidence: runner.${field} must be ${expected}`);
    }
  }
  const missing = missingRunnerObservation(runner);
  if (missing.length) {
    throw new Error(`${path} is not supported appliance timing evidence: runner observation is missing ${missing.join(', ')}`);
  }
}

function identityField(value: unknown, field: string): unknown {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? Reflect.get(value, field) : null;
}

function equalIdentity(actual: unknown, expected: unknown, at: string): void {
  for (const field of ['id', 'sha256']) {
    if ((identityField(actual, field) ?? null) !== (identityField(expected, field) ?? null)) {
      throw new Error(`${at}.${field} does not match the selected qualification scope`);
    }
  }
}

function equalIdentityFields(actual: unknown, expected: unknown, fields: readonly string[],
  at: string): void {
  for (const field of fields) {
    if ((identityField(actual, field) ?? null) !== (identityField(expected, field) ?? null)) {
      throw new Error(`${at}.${field} does not match the selected qualification scope`);
    }
  }
}

export function recommendPackBudgets({ binding, calibration, evidence }: {
  binding: RecipeBinding;
  calibration: CalibrationPlan;
  evidence: PackBudgetEvidence[];
}): PackBudgetResult {
  if (!binding?.release || !binding?.plan || !calibration) {
    throw new Error('pack budget recommendation requires a compiled recipe and calibration');
  }
  if (!Array.isArray(evidence) || evidence.length === 0) throw new Error('reference evidence is required');
  const calibrationIdentity = { id: calibration.id, sha256: calibration.contentSha256 };
  const expectedStacks = [...calibration.qualification.stacks].sort();
  const expectedPackCounts = new Map(binding.plan.packs.map(pack => [pack.id,
    binding.release.checkCatalog.filter(check => check.packId === pack.id).length]));
  const stacks = new Set<string>();
  const samples: PackRuntimeSample[] = [];
  let measuredEngine: ArtifactIdentity | null = null;
  let measuredRunner: RunnerObservation | null = null;

  for (const item of evidence) {
    const artifact = item.artifact;
    if (artifact?.kind !== 'reference_qualification') throw new Error(`${item.path} is not reference qualification evidence`);
    const payload = artifact.payload;
    if (payload.mutationControl !== false) throw new Error(`${item.path} is mutation evidence, not a pristine reference`);
    if (!payload.ok || !payload.stable || !payload.sameImage || !payload.sameHarness) {
      throw new Error(`${item.path} is not a passing stable reference qualification`);
    }
    equalIdentity(artifact.identities.recipe, { id: binding.release.id,
      sha256: binding.release.contentSha256 }, `${item.path}.identities.recipe`);
    equalIdentity(artifact.identities.calibration, calibrationIdentity,
      `${item.path}.identities.calibration`);
    equalIdentity(item.runtimeCalibration, { id: calibration.id,
      sha256: calibration.contentSha256 }, `${item.path}.retainedRuntimeCalibration`);
    const stack = artifact.identities.stackAdapter?.id;
    if (typeof stack !== 'string' || !expectedStacks.includes(stack)) {
      throw new Error(`${item.path} has unexpected stack ${stack ?? '<missing>'}`);
    }
    if (stacks.has(stack)) throw new Error(`reference evidence repeats stack ${stack}`);
    stacks.add(stack);
    const expectedReference = calibration.references.entries.find(entry => entry.backend === stack);
    if (!expectedReference) throw new Error(`${item.path} has unexpected stack ${stack}`);
    equalIdentity(artifact.identities.fixture, { id: expectedReference.id,
      sha256: expectedReference.sourceSha256 }, `${item.path}.identities.fixture`);
    if (payload.fixture !== expectedReference.id || payload.fixtureSha256 !== expectedReference.sourceSha256) {
      throw new Error(`${item.path} fixture does not match the selected ${stack} reference`);
    }
    if (payload.isolation !== 'docker') throw new Error(`${item.path} was not produced in Docker`);
    requireSupportedBudgetRunner(payload.runner, calibration.qualification.runner, item.path);
    if (measuredRunner === null) {
      if (!payload.runner) throw new Error(`${item.path} has no runner observation`);
      measuredRunner = payload.runner;
    }
    else if (canonicalDefinitionJson(payload.runner) !== canonicalDefinitionJson(measuredRunner)) {
      throw new Error(`${item.path} was measured on a different appliance runner environment`);
    }
    if (payload.requiredRepetitions !== calibration.qualification.referenceRepetitions
      || payload.runs?.length !== payload.requiredRepetitions) {
      throw new Error(`${item.path} does not contain the declared reference repetitions`);
    }
    if (measuredEngine === null) {
      if (!artifact.identities.engine) throw new Error(`${item.path} has no engine identity`);
      measuredEngine = artifact.identities.engine;
    }
    else equalIdentity(artifact.identities.engine, measuredEngine, `${item.path}.identities.engine`);

    const repetitions = new Set();
    for (const run of payload.runs) {
      if (!Number.isInteger(run.repetition) || run.repetition < 1
        || run.repetition > payload.requiredRepetitions || repetitions.has(run.repetition)) {
        throw new Error(`${item.path} has invalid or repeated repetition ${run.repetition}`);
      }
      repetitions.add(run.repetition);
      if (!run.ok) throw new Error(`${item.path} repetition ${run.repetition} did not pass`);
      const runtime = run.packRuntime;
      if (runtime?.schemaVersion !== 1 || runtime.metric !== PACK_RUNTIME_METRIC
        || !Array.isArray(runtime.packs)) {
        throw new Error(`${item.path} repetition ${run.repetition} has no supported pack runtime evidence`);
      }
      const seenPacks = new Set();
      for (const pack of runtime.packs) {
        if (!expectedPackCounts.has(pack.id)) throw new Error(`${item.path} measured unknown pack ${pack.id}`);
        if (seenPacks.has(pack.id)) throw new Error(`${item.path} repeats measured pack ${pack.id}`);
        seenPacks.add(pack.id);
        const checkCount = nonNegativeInteger(pack.checkCount,
          `${item.path}.${pack.id}.checkCount`);
        if (checkCount !== expectedPackCounts.get(pack.id)) {
          throw new Error(`${item.path} measured ${checkCount} checks for ${pack.id}; expected ${expectedPackCounts.get(pack.id)}`);
        }
        const setupRuntimeMs = nonNegativeInteger(pack.setupRuntimeMs,
          `${item.path}.${pack.id}.setupRuntimeMs`);
        const criterionRuntimeMs = nonNegativeInteger(pack.criterionRuntimeMs,
          `${item.path}.${pack.id}.criterionRuntimeMs`);
        const measuredRuntimeMs = nonNegativeInteger(pack.measuredRuntimeMs,
          `${item.path}.${pack.id}.measuredRuntimeMs`);
        if (measuredRuntimeMs !== setupRuntimeMs + criterionRuntimeMs) {
          throw new Error(`${item.path}.${pack.id}.measuredRuntimeMs does not equal its components`);
        }
        samples.push({ stack, repetition: run.repetition, packId: pack.id, checkCount,
          setupRuntimeMs, criterionRuntimeMs, measuredRuntimeMs });
      }
      const missing = [...expectedPackCounts.keys()].filter(id => !seenPacks.has(id));
      if (missing.length) throw new Error(`${item.path} repetition ${run.repetition} is missing packs: ${missing.join(', ')}`);
    }
  }
  const missingStacks = expectedStacks.filter(stack => !stacks.has(stack));
  if (missingStacks.length || stacks.size !== expectedStacks.length) {
    throw new Error(`reference evidence must cover each supported stack exactly once; missing ${missingStacks.join(', ') || 'none'}`);
  }
  if (!measuredEngine || !measuredRunner) throw new Error('reference evidence is incomplete');
  equalIdentity(measuredEngine, currentEngineIdentity(), 'reference evidence engine');
  samples.sort((a, b) => a.packId.localeCompare(b.packId)
    || a.stack.localeCompare(b.stack) || a.repetition - b.repetition);
  const recommendations = [...expectedPackCounts.keys()].sort().map(packId => {
    const observed = samples.filter(sample => sample.packId === packId).map(sample => sample.measuredRuntimeMs);
    const observedMaxRuntimeMs = Math.max(...observed);
    const maxRuntimeMs = Math.max(PACK_BUDGET_POLICY.minimumMs,
      Math.ceil(observedMaxRuntimeMs * PACK_BUDGET_POLICY.multiplier / PACK_BUDGET_POLICY.roundUpMs)
        * PACK_BUDGET_POLICY.roundUpMs);
    return { packId, sampleCount: observed.length,
      observedMinRuntimeMs: Math.min(...observed), observedMaxRuntimeMs, maxRuntimeMs };
  });
  return { measuredEngine, measuredRunner, samples, recommendations };
}

function containedRunPath(artifactPath: string, runOutput: unknown): string {
  if (typeof runOutput !== 'string' || !runOutput || isAbsolute(runOutput)) {
    throw new Error(`${artifactPath} contains an invalid absolute run output`);
  }
  const base = realpathSync(resolve(dirname(artifactPath)));
  const requested = resolve(base, runOutput);
  if (!existsSync(requested)) throw new Error(`${artifactPath} retained run is missing ${requested}`);
  const output = realpathSync(requested);
  if (output !== base && !output.startsWith(`${base}${sep}`)) {
    throw new Error(`${artifactPath} run output escapes its evidence directory`);
  }
  return output;
}

export function loadPackBudgetEvidence(paths: string[]): PackBudgetEvidence[] {
  return paths.map(path => {
    const artifact = readArtifact<ReferenceQualificationPayload>(path,
      { expectedKind: 'reference_qualification' });
    let runtimeCalibration: ArtifactIdentity | null = null;
    for (const run of artifact.payload.runs ?? []) {
      const output = containedRunPath(path, run.output);
      const bundlePath = resolve(output, 'grading', ARTIFACT_FILE.gradeBundle);
      const runPath = resolve(output, ARTIFACT_FILE.run);
      if (!existsSync(runPath)) throw new Error(`${path} retained run is missing ${runPath}`);
      if (!existsSync(bundlePath)) throw new Error(`${path} retained run is missing ${bundlePath}`);
      const raw = readArtifact(runPath, { expectedKind: 'benchmark_run' });
      const bundle = readArtifact<{ packRuntime: PackRuntime }>(bundlePath,
        { expectedKind: 'grade_bundle' });
      equalIdentity(raw.identities.engine, artifact.identities.engine,
        `${path} retained run ${run.repetition}.identities.engine`);
      equalIdentityFields(raw.identities.stackAdapter, artifact.identities.stackAdapter, ['id'],
        `${path} retained run ${run.repetition}.identities.stackAdapter`);
      equalIdentityFields(raw.identities.agentAdapter, { id: 'reference-fixture' }, ['id'],
        `${path} retained run ${run.repetition}.identities.agentAdapter`);
      for (const identity of ['engine', 'recipe', 'stackAdapter'] as const) {
        equalIdentity(bundle.identities[identity], artifact.identities[identity],
          `${path} retained run ${run.repetition}.identities.${identity}`);
      }
      equalIdentityFields(bundle.identities.calibration, artifact.identities.calibration,
        ['id', 'sha256'], `${path} retained run ${run.repetition}.identities.calibration`);
      if (runtimeCalibration === null) runtimeCalibration = bundle.identities.calibration;
      else equalIdentity(bundle.identities.calibration, runtimeCalibration,
        `${path} retained run ${run.repetition}.identities.calibration`);
      if (canonicalDefinitionJson(bundle.payload.packRuntime) !== canonicalDefinitionJson(run.packRuntime)) {
        throw new Error(`${path} pack runtime summary differs from retained run ${run.repetition}`);
      }
    }
    return { path, sha256: sha256(readFileSync(path)), artifact, runtimeCalibration };
  });
}
