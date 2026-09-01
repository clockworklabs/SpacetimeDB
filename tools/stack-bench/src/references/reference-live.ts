#!/usr/bin/env node
// Qualify fresh reference copies from audited evidence, not process exit codes.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { executeStackCapability } from '../stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeRunJson } from '../evidence/artifacts.js';
import { hashDirectory } from '../evidence/provenance.js';
import { inspectImportedReference, loadReferenceRegistry, prepareReferenceFixtureSource,
  validateReferenceRegistry } from './reference-fixtures.js';
import { resolveReferenceSelection } from './reference-selection.js';
import { auditMutationWorkerRun, auditReferenceRun }
  from './reference-qualification-audit.js';
import { rescueSupervisedLease } from '../runtime/recovery.js';
import { runBounded } from '../runtime/bounded-process.js';
import { calibrationQualificationIdentity, mutationExecutionSha256,
  resolveCalibrationForRelease } from '../composition/calibration-compiler.js';
import { qualificationScopeIdentity } from '../composition/qualification-scope.js';
import { resolveRecipeRelease } from '../composition/recipe-release.js';
import { isModularRecipeRelease } from '../composition/recipe-selection.js';
import { isDeclaredLevel, listTracks, loadTrack } from '../composition/tracks.js';
import { RUN_INDEX_CAP } from '../composition/tracks.js';
import { controllerRunner } from '../runtime/runner-environment.js';
import { mergeMutationShards, mutationShard, mutationWorkerSlots }
  from '../evidence/mutation-shards.js';
import { existingResourceLockKeys, resourceLockScope } from '../runtime/backend-lease.js';
import { resolveFeatureCatalog } from '../progression/feature-catalog-selection.js';
import { progressionLevels, selectFeatureCatalogLevels }
  from '../progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../progression/progression-recipe-selection.js';
import { mutationTargetKeys } from '../evidence/mutation-analysis.js';

export { controllerRunner as referenceQualificationRunner } from '../runtime/runner-environment.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../package-root.js';
const BENCH = compiledEntrypoint('commands', 'bench.js');
const DEFAULT_SPACETIME_PORT = 3310;

import type { ReferenceFixture } from './reference-fixtures.js';
import type { RecipeBinding } from '../composition/recipe-release.js';
import type { CalibrationPlan, CalibrationReference }
  from '../composition/calibration-compiler.js';
import type { ProgressionRecipeSelections }
  from '../progression/progression-recipe-selection.js';

// The flags a qualification run is launched with.
export interface ReferenceQualificationArgs {
  backend?: string;
  track: string;
  level: number;
  repetitions: number;
  runIndex: number;
  spacetimePort: number | null;
  timeoutMinutes: number | null;
  mutations: boolean;
  mutationWorkers: number;
  mutationShardIndex: number | null;
  mutationShardCount: number | null;
  mutationMaxRuntimeMinutes: number;
  mutationIds: string[];
  recipe?: string;
  featureCatalog?: string;
  out?: string;
  releaseCandidate?: boolean;
  artifactDirectory?: string;
  runsRoot?: string;
  timeoutMs?: number;
  spacetimePortExplicit?: boolean;
  mutationBaselineBundle?: string;
  mutationCheckpoint?: string | null;
  mutationCheckpointDir?: string;
  referenceMutationOnly?: boolean;
}

type UnknownRecord = Record<string, unknown>;

// What a qualification run resolves once and threads through every step.
export interface QualificationContext {
  binding: RecipeBinding;
  calibration: CalibrationPlan;
  reference?: CalibrationReference;
  identity?: unknown;
  featureCatalog?: unknown;
  featureCatalogRef?: string | null;
  progressionSelection?: ProgressionRecipeSelections | null;
  selectedCheckKeys?: string[];
  level?: number;
}

export interface MutationManifest {
  mutations: UnknownRecord[];
  scenario?: string;
  [key: string]: unknown;
}

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object';

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);


interface QualificationRunRecord {
  ok?: boolean;
  fingerprint?: string;
  imageId?: string;
  output?: string;
  [key: string]: unknown;
}

interface QualificationArtifact {
  runs: QualificationRunRecord[];
  ok?: boolean;
  [key: string]: unknown;
}

interface MutationShardControl {
  ok?: boolean;
  results?: Array<{ id?: unknown; [key: string]: unknown }>;
  shard?: { index?: number; count?: number; mutationIds?: string[] };
  [key: string]: unknown;
}

export interface MutationWorkerResult {
  artifact: unknown;
  payload: WorkerPayload | null;
  run: WorkerRun | null;
  control: MutationShardControl | null;
  assigned: string[];
  shardVerified: boolean;
  failures: string[];
}

interface WorkerRun {
  ok?: boolean;
  output?: string;
  [key: string]: unknown;
}

interface WorkerPayload {
  ok?: boolean;
  mutationControl?: boolean;
  requiredRepetitions?: number;
  runs?: WorkerRun[];
  [key: string]: unknown;
}

function qualificationInputs(): { sha256: string; files: string[] } {
  const ignoredRoots = new Set([
    'archive',
    'dist',
    'local-notes',
    'media',
    'node_modules',
    'qualification-evidence',
    'reference-apps',
    'results',
    'tests',
    'transcripts',
  ]);
  return hashDirectory(ROOT, { exclude: (name, entry) => {
    const parts = name.split('/');
    if (ignoredRoots.has(parts[0] ?? '') || parts.some(part => part.startsWith('.'))) return true;
    if (entry.isDirectory()) return false;
    return !(/\.(?:mjs|ts|js|json|ya?ml|sh)$/.test(name) || /(?:^|\/)Dockerfile$/.test(name));
  } });
}

export function parseReferenceQualificationArgs(argv: readonly string[]):
  ReferenceQualificationArgs {
  const { values } = parseNodeArgs({ args: [...argv.slice(2)], options: {
    backend: { type: 'string' }, track: { type: 'string' }, level: { type: 'string' },
    recipe: { type: 'string' }, 'feature-catalog': { type: 'string' },
    repetitions: { type: 'string' }, 'run-index': { type: 'string' },
    'spacetime-port': { type: 'string' }, 'timeout-minutes': { type: 'string' },
    mutations: { type: 'boolean' }, 'release-candidate': { type: 'boolean' },
    'mutation-id': { type: 'string', multiple: true }, 'mutation-workers': { type: 'string' },
    'mutation-shard-index': { type: 'string' }, 'mutation-shard-count': { type: 'string' },
    'mutation-checkpoint-dir': { type: 'string' }, 'mutation-checkpoint': { type: 'string' },
    'mutation-baseline-bundle': { type: 'string' }, 'mutation-max-runtime-minutes': { type: 'string' },
    'reference-mutation-only': { type: 'boolean' }, out: { type: 'string' },
  } });
  const number = (value: string | undefined, fallback: number | null): number | null =>
    value === undefined ? fallback : Number(value);
  const args: ReferenceQualificationArgs = {
    backend: values.backend, track: values.track ?? 'ecommerce',
    level: number(values.level, 1) as number, recipe: values.recipe,
    featureCatalog: values['feature-catalog'], repetitions: number(values.repetitions, 2) as number,
    runIndex: number(values['run-index'], 0) as number,
    spacetimePort: number(values['spacetime-port'], null),
    spacetimePortExplicit: values['spacetime-port'] !== undefined,
    timeoutMinutes: number(values['timeout-minutes'], null),
    mutations: values.mutations ?? false, releaseCandidate: values['release-candidate'],
    mutationIds: values['mutation-id'] ?? [], mutationWorkers: number(values['mutation-workers'], 1) as number,
    mutationShardIndex: number(values['mutation-shard-index'], null),
    mutationShardCount: number(values['mutation-shard-count'], null),
    mutationCheckpointDir: values['mutation-checkpoint-dir'] && resolve(values['mutation-checkpoint-dir']),
    mutationCheckpoint: values['mutation-checkpoint'] && resolve(values['mutation-checkpoint']),
    mutationBaselineBundle: values['mutation-baseline-bundle'] && resolve(values['mutation-baseline-bundle']),
    mutationMaxRuntimeMinutes: number(values['mutation-max-runtime-minutes'], 60) as number,
    referenceMutationOnly: values['reference-mutation-only'],
    out: values.out && resolve(values.out),
  };
  if (args.backend === undefined || !STACK_ADAPTER_REGISTRY.ids.includes(args.backend)) {
    throw new Error(`--backend must be one of ${STACK_ADAPTER_REGISTRY.ids.join(', ')}`);
  }
  if (!listTracks().includes(args.track)) throw new Error(`--track is unknown: ${args.track}`);
  const track = loadTrack(args.track);
  if (!isDeclaredLevel(track, args.level)) {
    throw new Error(`--level must be declared for ${args.track}`);
  }
  if (!Number.isInteger(args.repetitions) || args.repetitions < 1) {
    throw new Error('--repetitions must be a positive integer');
  }
  if (!Number.isInteger(args.runIndex) || args.runIndex < 0) {
    throw new Error('--run-index must be a non-negative integer');
  }
  if (!Number.isInteger(args.mutationWorkers) || args.mutationWorkers < 1
      || args.mutationWorkers > 8) {
    throw new Error('--mutation-workers must be an integer from 1 through 8');
  }
  if (args.mutationWorkers > 1 && !args.mutations) {
    throw new Error('--mutation-workers above 1 requires --mutations');
  }
  if ((args.mutationCheckpointDir || args.mutationCheckpoint || args.mutationBaselineBundle)
      && !args.mutations) {
    throw new Error('mutation control options require --mutations');
  }
  if (args.mutationCheckpoint && args.mutationWorkers !== 1) {
    throw new Error('--mutation-checkpoint is an internal single-worker option');
  }
  if (args.referenceMutationOnly && (!args.mutations || args.mutationWorkers !== 1)) {
    throw new Error('--reference-mutation-only is an internal single-worker option');
  }
  if (args.releaseCandidate && !args.mutations) {
    throw new Error('--release-candidate requires --mutations');
  }
  if (args.mutationIds.some(id => typeof id !== 'string' || !id.trim())
      || new Set(args.mutationIds).size !== args.mutationIds.length) {
    throw new Error('--mutation-id values must be unique non-empty strings');
  }
  if (args.mutationIds.length && !args.mutations) {
    throw new Error('--mutation-id requires --mutations');
  }
  if (args.releaseCandidate && args.mutationIds.length) {
    throw new Error('--release-candidate cannot select individual mutations');
  }
  if (args.mutations && !args.referenceMutationOnly && !args.releaseCandidate
      && args.mutationIds.length === 0) {
    throw new Error('full mutation qualification requires --release-candidate');
  }
  if (args.mutationBaselineBundle && !args.referenceMutationOnly) {
    throw new Error('--mutation-baseline-bundle is an internal mutation-worker option');
  }
  if (!Number.isFinite(args.mutationMaxRuntimeMinutes)
      || args.mutationMaxRuntimeMinutes < 1 || args.mutationMaxRuntimeMinutes > 120) {
    throw new Error('--mutation-max-runtime-minutes must be from 1 through 120');
  }
  const shardSupplied = args.mutationShardIndex !== null || args.mutationShardCount !== null;
  if (shardSupplied && (args.mutationShardIndex === null || args.mutationShardCount === null)) {
    throw new Error('--mutation-shard-index and --mutation-shard-count must be supplied together');
  }
  if (shardSupplied) {
    if (!args.mutations || args.mutationWorkers !== 1) {
      throw new Error('internal mutation shard coordinates require --mutations and one worker');
    }
    // The pure partition helper owns coordinate range validation.
    mutationWorkerSlots({ workerCount: args.mutationShardCount ?? 0, runIndex: 0,
      maxRunIndex: RUN_INDEX_CAP });
    if (!Number.isInteger(args.mutationShardIndex) || Number(args.mutationShardIndex) < 0
        || Number(args.mutationShardIndex) >= Number(args.mutationShardCount)) {
      throw new Error('--mutation-shard-index is outside the declared shard count');
    }
  } else {
    mutationWorkerSlots({ workerCount: args.mutationWorkers, runIndex: args.runIndex,
      maxRunIndex: RUN_INDEX_CAP });
  }
  args.spacetimePort ??= DEFAULT_SPACETIME_PORT + args.runIndex;
  if (!Number.isInteger(args.spacetimePort) || args.spacetimePort < 1024 || args.spacetimePort > 65535) {
    throw new Error('--spacetime-port must be an integer from 1024 through 65535');
  }
  if (args.mutations && args.spacetimePort + args.mutationWorkers - 1 > 65535) {
    throw new Error('--spacetime-port plus mutation worker offsets must not exceed 65535');
  }
  args.timeoutMinutes ??= args.mutations ? 120 : 60;
  const maximumTimeoutMinutes = args.mutations ? 180 : 240;
  if (!Number.isFinite(args.timeoutMinutes) || args.timeoutMinutes < 10
      || args.timeoutMinutes > maximumTimeoutMinutes) {
    throw new Error(`--timeout-minutes must be from 10 through ${maximumTimeoutMinutes}`);
  }
  if (args.mutations && args.timeoutMinutes < args.mutationMaxRuntimeMinutes + 20) {
    throw new Error('--timeout-minutes must allow the mutation batch plus 20 minutes for setup');
  }
  args.timeoutMs = Math.round(args.timeoutMinutes * 60_000);
  return args;
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function referenceQualificationContext(fixture: ReferenceFixture,
  recipe: string | null = null,
  { level = fixture.level, featureCatalog = null }: {
    level?: number; featureCatalog?: string | null;
  } = {}): QualificationContext {
  const track = loadTrack(fixture.track);
  const binding = resolveRecipeRelease(track, level, recipe);
  if (!binding) throw new Error(`${fixture.track} L${level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, stackBenchRoot: ROOT, alias: `L${level}` });
  if (!calibration) {
    throw new Error(`${binding.release.id}@${binding.release.version} has no L${level} calibration`);
  }
  const reference = calibration.references.entries.find(entry => entry.backend === fixture.backend
    && entry.id === fixture.id && entry.sourceSha256 === fixture.imported?.sourceSha256);
  if (!reference) throw new Error(`${fixture.id} is not selected by calibration ${calibration.id}`);
  const declaredCatalog = calibration.qualification.featureCatalog;
  const catalogRef = featureCatalog ?? (declaredCatalog
    ? `${declaredCatalog.id}@${declaredCatalog.version}` : null);
  const fullCatalog = catalogRef ? resolveFeatureCatalog(catalogRef, track) : null;
  const catalog = fullCatalog ? selectFeatureCatalogLevels(fullCatalog,
    progressionLevels(fullCatalog).filter(candidate => candidate <= level)) : null;
  if (declaredCatalog && catalog?.identity.sha256 !== declaredCatalog.sha256) {
    throw new Error(`${calibration.id} feature catalog identity does not match`);
  }
  const progressionSelection = catalog
    ? resolveProgressionRecipeLevelSelection(binding, catalog, level, { cumulative: true }) : null;
  const selectedCheckKeys = progressionSelection?.grader.checkKeys
    ?? binding.release.checkCatalog.map(check => check.stableKey);
  return { binding, calibration, identity: calibrationQualificationIdentity(calibration),
    featureCatalog: catalog?.identity ?? null, featureCatalogRef: catalogRef,
    progressionSelection, selectedCheckKeys, level };
}

export function referenceQualificationSelectionArgs(binding: RecipeBinding | null,
  progressionSelection: ProgressionRecipeSelections | null = null,
  selectedCheckKeys: string[] | null = null): string[] {
  if (!binding?.release?.checkCatalog?.length) {
    throw new Error('reference qualification requires an exact recipe check catalog');
  }
  const selected = progressionSelection?.grader.selection ?? null;
  const checkKeys = selectedCheckKeys
    ?? progressionSelection?.grader.checkKeys
    ?? binding.release.checkCatalog.map(check => check.stableKey);
  const args = ['--check', checkKeys.join(',')];
  if (!isModularRecipeRelease(binding.release)) return args;
  const features = selected?.requested.features ?? binding.release.components.packs
    .filter(pack => pack.moduleType === 'feature').map(pack => pack.id);
  const specifications = selected?.requested.specifications.expected
    ?? binding.release.components.packs.filter(pack => pack.moduleType === 'specification')
      .map(pack => `${pack.id}@${pack.version}`);
  if (!features.length || !specifications.length) {
    throw new Error('modular reference qualification requires feature and specification modules');
  }
  const requestTask = progressionSelection?.grader.request.task;
  const taskMode = record(requestTask) ? requestTask.mode : undefined;
  if (progressionSelection
    && (typeof taskMode !== 'string' || !['fresh', 'upgrade'].includes(taskMode))) {
    throw new Error('progression reference qualification requires a fresh or upgrade task mode');
  }
  return ['--feature-module', features.join(','), '--expect-spec', specifications.join(','),
    ...(taskMode ? ['--task-mode', String(taskMode)] : []), ...args];
}

export function referenceQualificationRelease<T extends { checkCatalog: Array<{ stableKey: string }> }>(
  release: T, selectedCheckKeys: readonly string[]): T {
  const requested = new Set(selectedCheckKeys);
  if (requested.size !== selectedCheckKeys.length) {
    throw new Error('reference qualification check selection contains duplicates');
  }
  const checkCatalog = release.checkCatalog.filter(check => requested.delete(check.stableKey));
  if (requested.size) {
    throw new Error(`reference qualification selected unknown checks: ${
      [...requested].sort().join(', ')}`);
  }
  if (checkCatalog.length === 0) throw new Error('reference qualification selected no checks');
  return { ...release, checkCatalog };
}

export function referenceQualificationPaths(args: Pick<ReferenceQualificationArgs, 'out'>, id: string):
  { artifactPath: string; artifactDirectory: string; runsRoot: string } {
  const artifactPath = args.out ?? join(ROOT, 'results', 'reference-live', `${id}.json`);
  const artifactName = basename(artifactPath, extname(artifactPath));
  return {
    artifactPath,
    artifactDirectory: dirname(artifactPath),
    runsRoot: join(dirname(artifactPath), `${artifactName}.runs`),
  };
}

export function companionReferenceArtifactPath(mutationArtifactPath: string): string {
  const extension = extname(mutationArtifactPath) || '.json';
  const stem = basename(mutationArtifactPath, extension);
  const referenceStem = stem.endsWith('-mutation')
    ? `${stem.slice(0, -'-mutation'.length)}-reference`
    : `${stem}-reference`;
  return join(dirname(mutationArtifactPath), `${referenceStem}${extension}`);
}

export function assertReleaseCandidateRepetitions(args: Pick<ReferenceQualificationArgs,
  'releaseCandidate' | 'repetitions'>,
  calibration: { qualification?: { mutationRepetitions?: number } } | null): void {
  if (!args.releaseCandidate) return;
  const required = calibration?.qualification?.mutationRepetitions;
  if (!Number.isInteger(required) || Number(required) < 1) {
    throw new Error('release calibration has no valid mutation repetition count');
  }
  if (args.repetitions !== required) {
    throw new Error(`--release-candidate requires exactly ${required} mutation repetition(s)`);
  }
}

export function referenceQualificationWorkRoot(env: NodeJS.ProcessEnv = process.env): string {
  return resolve(env.STACK_BENCH_WORK_DIR ?? tmpdir());
}

export function qualificationMutationManifest(fixture: ReferenceFixture,
  context: { calibration: { mutations: Array<{ backend: string; path: string;
    targets: Array<{ id: string }> }> } },
  requestedIds: readonly string[] = []): MutationManifest {
  const selection = context.calibration.mutations.find(entry => entry.backend === fixture.backend);
  if (!selection) throw new Error(`${fixture.id} has no mutation selection in its calibration`);
  if (!(fixture.mutationManifests ?? []).includes(selection.path)) {
    throw new Error(`${fixture.id} does not own its calibrated mutation manifest: ${selection.path}`);
  }
  const manifest = readJson(join(ROOT, selection.path)) as MutationManifest;
  const selectedIds = new Set(selection.targets.map(target => target.id));
  const mutations = manifest.mutations.filter(mutation =>
    selectedIds.delete(String(mutation.id)));
  if (selectedIds.size) {
    throw new Error(`${fixture.id} mutation selection is missing: ${[...selectedIds].sort().join(', ')}`);
  }
  if (mutations.length === 0) throw new Error(`${fixture.id} mutation selection is empty`);
  if (requestedIds.length === 0) return { ...manifest, mutations };
  const requested = new Set(requestedIds);
  const targeted = mutations.filter(mutation => requested.delete(String(mutation.id)));
  if (requested.size) {
    throw new Error(`${fixture.id} targeted mutation selection is missing: ${[...requested].sort().join(', ')}`);
  }
  return { ...manifest, mutations: targeted };
}

export function targetedMutationCheckKeys(context: {
  binding: { release: { checkCatalog: Array<{ stableKey: string; points: number }> } };
  selectedCheckKeys?: string[];
},
  manifest: MutationManifest): string[] {
  const available = new Map(context.binding.release.checkCatalog.map(check =>
    [check.stableKey, check]));
  const allowed = new Set(Array.isArray(context.selectedCheckKeys)
    ? context.selectedCheckKeys : []);
  const requested = new Set(manifest.mutations.flatMap(mutationTargetKeys));
  const missing = [...requested].filter(key => !available.has(key));
  if (missing.length) {
    throw new Error(`targeted mutations name unknown checks: ${missing.sort().join(', ')}`);
  }
  const outsideScope = [...requested].filter(key => !allowed.has(key)
    && Number(available.get(key)?.points) > 0);
  if (outsideScope.length) {
    throw new Error(`targeted mutations name checks outside the run scope: ${
      outsideScope.sort().join(', ')}`);
  }
  const selected = context.binding.release.checkCatalog
    .filter(check => Number(check.points) > 0 && allowed.has(check.stableKey)
      && requested.has(check.stableKey))
    .map(check => check.stableKey);
  if (selected.length === 0) throw new Error('targeted mutations select no scored checks');
  return selected;
}

async function runOnce(fixture: ReferenceFixture, args: ReferenceQualificationArgs,
  context: QualificationContext, id: string, repetition: number): Promise<UnknownRecord> {
  const workRoot = referenceQualificationWorkRoot();
  mkdirSync(workRoot, { recursive: true });
  const work = mkdtempSync(join(workRoot, `reference-live-${fixture.backend}-`));
  const app = join(work, 'app');
  const output = join(String(args.runsRoot), `r${repetition + 1}`);
  const supervisorState = join(work, 'supervisor-state.json');
  const started = Date.now();
  const harnessBefore = qualificationInputs();
  const cancellation = new AbortController();
  const cancel = () => cancellation.abort();
  process.once('SIGINT', cancel);
  process.once('SIGTERM', cancel);
  let processError = null;
  try {
    prepareReferenceFixtureSource(fixture, app);
    const adapter = STACK_ADAPTER_REGISTRY.get(fixture.backend);
    const supervisorEnv = executeStackCapability(adapter, 'run-policy', 'supervisor-env',
      { spacetimePort: args.spacetimePort });
    const env = { ...process.env, STACK_BENCH_SUPERVISOR_STATE: supervisorState,
      ...(record(supervisorEnv) ? supervisorEnv : {}) };
    const benchArgs = [BENCH, '--backend', fixture.backend, '--track', fixture.track,
      '--levels', String(args.level), '--run-index', String(args.runIndex), '--fix-rounds', '0',
      '--app', app, '--out', output, '--agent-adapter', 'reference-fixture', '--no-media'];
    benchArgs.push('--recipe', `${context.binding.release.id}@${context.binding.release.version}`);
    benchArgs.push(...referenceQualificationSelectionArgs(context.binding,
      context.progressionSelection, context.selectedCheckKeys ?? null));
    benchArgs.push('--parent-attempt-id', id);
    if (args.mutations) {
      const manifestPath = join(work, 'selected-mutations.json');
      writeFileSync(manifestPath, `${JSON.stringify(qualificationMutationManifest(fixture, context,
        args.mutationIds),
        null, 2)}\n`);
      benchArgs.push('--mutations', manifestPath);
      if (args.mutationShardCount !== null) {
        benchArgs.push('--mutation-shard-index', String(args.mutationShardIndex),
          '--mutation-shard-count', String(args.mutationShardCount));
      }
      if (args.mutationCheckpoint) {
        if (existsSync(String(args.mutationCheckpoint))) {
          benchArgs.push('--mutation-resume-from', String(args.mutationCheckpoint));
        }
        benchArgs.push('--mutation-checkpoint-out', String(args.mutationCheckpoint));
      }
      benchArgs.push('--mutation-max-runtime-minutes', String(args.mutationMaxRuntimeMinutes));
      benchArgs.push('--expected-mutation-calibration-json', JSON.stringify({
        id: context.calibration.id,
        version: context.calibration.version,
        sha256: context.calibration.contentSha256,
        state: context.calibration.state,
      }));
      if (args.mutationBaselineBundle) {
        benchArgs.push('--mutation-baseline-bundle', String(args.mutationBaselineBundle));
      }
      if (args.referenceMutationOnly) benchArgs.push('--reference-mutation-only');
    }
    const child = await runBounded(process.execPath, benchArgs,
      { cwd: ROOT, stdio: 'inherit', env, timeoutMs: Number(args.timeoutMs),
        signal: cancellation.signal });
    if (!child.ok) {
      const reason = child.timedOut
        ? `benchmark exceeded ${args.timeoutMinutes} minute repetition deadline`
        : child.cancelled ? 'benchmark was interrupted'
        : child.error?.message ?? `benchmark exited ${child.code ?? child.signal ?? 'without status'}`;
      throw new Error(reason);
    }
  } catch (error) {
    processError = errorMessage(error);
  } finally {
    process.removeListener('SIGINT', cancel);
    process.removeListener('SIGTERM', cancel);
  }
  let audit: UnknownRecord & { ok: boolean; failures: string[] };
  try {
    audit = args.referenceMutationOnly
      ? auditMutationWorkerRun(output, fixture)
      : auditReferenceRun(output, fixture, {
        requireMutationControl: args.mutations,
        release: context.binding.release,
        level: args.level,
        selectedCheckKeys: context.selectedCheckKeys,
      });
  } catch (error) {
    audit = { ok: false, failures: [`qualification evidence is invalid: ${errorMessage(error)}`] };
  }
  let cleanupPending = false;
  try { rescueSupervisedLease(supervisorState, output); }
  catch (error) {
    cleanupPending = true;
    audit.failures.push(`lease cleanup failed: ${errorMessage(error)}; recovery authority retained at ${supervisorState}`);
  }
  if (!cleanupPending) {
    try { rmSync(work, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 }); }
    catch (error) {
      const code = record(error) ? error.code : undefined;
      audit.failures.push(`owned temp cleanup failed (${String(code ?? 'unknown')}): ${work}`);
    }
  }
  const harnessAfter = qualificationInputs();
  if (harnessAfter.sha256 !== harnessBefore.sha256) {
    audit.failures.unshift('qualification harness changed while this repetition was running');
  }
  if (processError) audit.failures.unshift(`benchmark process failed: ${processError}`);
  audit.ok = audit.failures.length === 0;
  return { repetition: repetition + 1,
    output: relative(String(args.artifactDirectory), output).replaceAll('\\', '/'),
    durationMs: Date.now() - started, processError,
    harnessSha256Before: harnessBefore.sha256, harnessSha256After: harnessAfter.sha256, ...audit };
}

export function parallelMutationChildArgv(args: ReferenceQualificationArgs,
  context: { binding: { release: { id: string; version: string } }; featureCatalogRef?: string | null },
  { artifactPath, baselineBundle, workerIndex, workerCount }: {
    artifactPath: string; baselineBundle: string;
    workerIndex: number; workerCount: number;
  }): string[] {
  const runIndex = args.runIndex + workerIndex;
  const argv: string[] = [fileURLToPath(import.meta.url), '--backend', String(args.backend),
    '--track', args.track, '--level', String(args.level), '--recipe',
    `${context.binding.release.id}@${context.binding.release.version}`,
    '--repetitions', '1', '--run-index', String(runIndex), '--timeout-minutes',
    String(args.timeoutMinutes), '--mutations', '--mutation-shard-index',
    String(workerIndex), '--mutation-shard-count', String(workerCount), '--out', artifactPath];
  argv.push('--reference-mutation-only');
  argv.push('--mutation-max-runtime-minutes', String(args.mutationMaxRuntimeMinutes));
  argv.push('--mutation-baseline-bundle', baselineBundle);
  if (args.mutationCheckpointDir) {
    argv.push('--mutation-checkpoint', join(String(args.mutationCheckpointDir),
      `${args.backend}-worker-${workerIndex + 1}.json`));
  }
  if (context.featureCatalogRef) {
    argv.push('--feature-catalog', String(context.featureCatalogRef));
  }
  for (const mutationId of args.mutationIds ?? []) {
    argv.push('--mutation-id', String(mutationId));
  }
  if (args.spacetimePortExplicit) {
    argv.push('--spacetime-port', String(Number(args.spacetimePort) + workerIndex));
  }
  return argv;
}

export function parallelMutationResourceLockKeys(args: ReferenceQualificationArgs): string[] {
  const slots = mutationWorkerSlots({ workerCount: args.mutationWorkers,
    runIndex: args.runIndex, maxRunIndex: RUN_INDEX_CAP });
  const keys = slots.map(runIndex => `slot:${args.track}:${args.backend}:run${runIndex}`);
  if (args.backend === 'spacetime') {
    keys.push(...slots.map((_, workerIndex) =>
      `listener:http://127.0.0.1:${Number(args.spacetimePort) + workerIndex}`));
  }
  return keys.sort();
}

export function preflightParallelMutationResources(args: ReferenceQualificationArgs,
  env: NodeJS.ProcessEnv = process.env): void {
  const occupied = existingResourceLockKeys({
    root: resourceLockScope(env).root,
    keys: parallelMutationResourceLockKeys(args),
  });
  if (occupied.length) {
    throw new Error(`parallel mutation resources are already leased: ${occupied.join(', ')}`);
  }
}

function identityKey(identity: unknown): string {
  const value = record(identity) ? identity : {};
  return JSON.stringify({ id: value.id ?? null, version: value.version ?? null,
    sha256: value.sha256 ?? null, state: value.state ?? null });
}

export function readParallelMutationWorker(path: string, processResult: {
  ok: boolean; timedOut?: boolean; error?: Error | null; code?: number | null; signal?: NodeJS.Signals | null;
},
  expected: { workerIndex: number; workerCount: number; [key: string]: unknown },
  manifest: MutationManifest): MutationWorkerResult {
  const failures = [];
  const assigned = mutationShard(manifest.mutations, { index: expected.workerIndex,
    count: expected.workerCount, defaultScenario: manifest.scenario }).mutationIds;
  if (!processResult.ok) {
    failures.push(processResult.timedOut ? 'worker timed out'
      : processResult.error?.message ?? `worker exited ${processResult.code ?? processResult.signal}`);
  }
  if (!existsSync(path)) return { artifact: null, payload: null, run: null, control: null,
    assigned, shardVerified: false, failures: [...failures, 'worker artifact is missing'] };
  let artifact: ReturnType<typeof readArtifact<WorkerPayload>>;
  try { artifact = readArtifact<WorkerPayload>(path); }
  catch (error) { return { artifact: null, payload: null, run: null, control: null,
    assigned, shardVerified: false,
    failures: [...failures, `worker artifact is invalid: ${errorMessage(error)}`] }; }
  if (artifact.kind !== 'reference_qualification') failures.push('worker artifact has the wrong kind');
  const payload = artifact.payload;
  if (payload?.mutationControl !== true || payload?.requiredRepetitions !== 1
      || payload?.runs?.length !== 1) {
    failures.push('worker qualification shape is invalid');
  }
  const identities: UnknownRecord = record(artifact.identities) ? artifact.identities : {};
  for (const key of ['engine', 'fixture', 'recipe', 'calibration', 'stackAdapter']) {
    if (identityKey(identities[key]) !== identityKey(expected[key])) {
      failures.push(`worker ${key} identity does not match the parent`);
    }
  }
  const run = payload?.runs?.[0] ?? null;
  if (payload?.ok !== true || run?.ok !== true) failures.push('worker qualification did not pass');
  let control: MutationShardControl | null = null;
  if (run?.output) {
    const childRoot = resolve(dirname(path));
    const outputRoot = resolve(childRoot, run.output);
    if (outputRoot !== childRoot && !outputRoot.startsWith(`${childRoot}${sep}`)) {
      failures.push('worker run output escapes its artifact directory');
    } else {
      const controlPath = join(outputRoot, 'mutation-control.json');
      try { control = readArtifactPayload(controlPath, { expectedKind: 'mutation_control' }); }
      catch (error) {
        failures.push(`worker mutation artifact is invalid: ${errorMessage(error)}`);
      }
    }
  } else failures.push('worker run output is missing');
  const shardMatches = control?.shard?.index === expected.workerIndex
    && control.shard.count === expected.workerCount
    && JSON.stringify(control.shard.mutationIds) === JSON.stringify(assigned);
  if (!control?.shard) failures.push('worker mutation shard is missing');
  else if (!shardMatches) failures.push('worker mutation shard does not match its assignment');
  const resultIds = Array.isArray(control?.results)
    ? control.results.map((result: UnknownRecord) => String(result.id ?? '')).sort() : null;
  const assignedIds = [...assigned].sort();
  const resultsMatch = resultIds !== null && resultIds.length === assignedIds.length
    && JSON.stringify(resultIds) === JSON.stringify(assignedIds);
  if (!Array.isArray(control?.results)) failures.push('worker mutation results are missing');
  else if (!resultsMatch) failures.push('worker mutation results do not match their shard assignment');
  if (control?.ok !== true) failures.push('worker mutation control did not pass');
  return { artifact, payload, run, control, assigned,
    shardVerified: shardMatches && resultsMatch, failures };
}

export function parallelMutationResults(manifest: MutationManifest,
  workers: ReadonlyArray<{ shardVerified: boolean; control: MutationShardControl | null }>): UnknownRecord[] {
  const completed = workers.filter(worker =>
    worker.shardVerified && Array.isArray(worker.control?.results));
  if (completed.length !== workers.length) {
    const byId = new Map(completed.flatMap(worker => worker.control?.results ?? [])
      .map(result => [String(result.id ?? ''), result]));
    return manifest.mutations.map(mutation => byId.get(String(mutation.id)))
      .filter((result): result is UnknownRecord => result !== undefined);
  }
  return mergeMutationShards(manifest.mutations, completed.map(worker => ({
    index: Number(worker.control?.shard?.index),
    count: Number(worker.control?.shard?.count),
    mutationIds: worker.control?.shard?.mutationIds ?? [],
    results: worker.control?.results ?? [],
  })), { defaultScenario: manifest.scenario });
}

export function mutationWorkerRequiresSiblingAbort(processResult: {
  ok: boolean; timedOut?: boolean; error?: Error | null; code?: number | null; signal?: NodeJS.Signals | null;
},
  worker: { control: MutationShardControl | null; assigned: unknown } | null): boolean {
  const outcome = record(worker?.control?.outcome) ? worker.control.outcome : {};
  if (outcome.kind === 'harness_failure') return true;
  if (processResult.ok) return false;
  const checkpoint = record(worker?.control?.checkpoint) ? worker.control.checkpoint : {};
  const assigned = Array.isArray(worker?.assigned) ? worker.assigned : [];
  const completed = checkpoint.status === 'complete'
    && Array.isArray(worker?.control?.results)
    && worker.control.results.length === assigned.length;
  return !completed;
}

async function runParallelMutationRepetition(fixture: ReferenceFixture,
  args: ReferenceQualificationArgs, context: QualificationContext, id: string,
  repetition: number, artifactIdentities: UnknownRecord,
  onCleanBaseline: ((clean: UnknownRecord) => void) | null = null): Promise<UnknownRecord> {
  const manifest = qualificationMutationManifest(fixture, context, args.mutationIds);
  if (!Array.isArray(manifest.mutations)
      || manifest.mutations.length < args.mutationWorkers) {
    throw new Error(`--mutation-workers cannot exceed ${manifest.mutations?.length ?? 0} mutations`);
  }
  const started = Date.now();
  const clean = await runOnce(fixture, { ...args, mutations: false, mutationWorkers: 1,
    referenceMutationOnly: false, mutationCheckpoint: null }, context, id, repetition);
  if (!clean.ok) {
    return { ...clean, durationMs: Date.now() - started,
      failures: (Array.isArray(clean.failures) ? clean.failures : [])
        .map(failure => `clean baseline: ${String(failure)}`),
      mutations: { caught: 0, total: 0 } };
  }
  const baselineBundle = resolve(String(args.artifactDirectory), String(clean.output),
    `first-build-l${args.level}-grading`, 'bundle.json');
  if (!existsSync(baselineBundle)) {
    return { ...clean, ok: false, durationMs: Date.now() - started,
      processError: `clean baseline bundle is missing: ${baselineBundle}`,
      failures: [`clean baseline bundle is missing: ${baselineBundle}`],
      outcome: 'incomplete', mutations: { caught: 0, total: 0 } };
  }
  if (onCleanBaseline) onCleanBaseline(clean);
  const remainingMs = started + Number(args.timeoutMs) - Date.now();
  if (remainingMs <= 0) {
    return { ...clean, ok: false, durationMs: Date.now() - started,
      processError: 'mutation qualification exhausted its repetition deadline after the clean baseline',
      failures: ['mutation qualification exhausted its repetition deadline after the clean baseline'],
      outcome: 'incomplete', mutations: { caught: 0, total: 0 } };
  }
  preflightParallelMutationResources(args);
  const workerRoot = join(String(args.runsRoot), `r${repetition + 1}-workers`);
  mkdirSync(workerRoot, { recursive: true });
  const cancellation = new AbortController();
  let cancellationReason = null;
  const cancel = () => {
    cancellationReason ??= 'parallel mutation qualification was interrupted';
    cancellation.abort();
  };
  process.once('SIGINT', cancel);
  process.once('SIGTERM', cancel);
  let workers;
  try {
    workers = await Promise.all(Array.from({ length: args.mutationWorkers }, async (_, workerIndex) => {
      const artifactPath = join(workerRoot, `w${workerIndex + 1}.json`);
      const logs = { stdout: join(workerRoot, `w${workerIndex + 1}.stdout.log`),
        stderr: join(workerRoot, `w${workerIndex + 1}.stderr.log`) };
      const argv = parallelMutationChildArgv(args, context,
        { artifactPath, baselineBundle, workerIndex,
          workerCount: args.mutationWorkers });
      const processResult = await runBounded(process.execPath, argv,
        { cwd: ROOT, env: process.env, timeoutMs: remainingMs, logs,
          signal: cancellation.signal, gracefulCancellationMs: 10_000 });
      const worker = { workerIndex, runIndex: args.runIndex + workerIndex,
        artifactPath, logs, processResult };
      const inspected = readParallelMutationWorker(worker.artifactPath, processResult,
        { ...artifactIdentities, workerIndex, workerCount: args.mutationWorkers }, manifest);
      if (mutationWorkerRequiresSiblingAbort(processResult, inspected)) {
        cancellationReason ??= `worker ${workerIndex + 1} failed before usable mutation evidence`;
        cancellation.abort();
      }
      return { ...worker, ...inspected };
    }));
  } finally {
    process.removeListener('SIGINT', cancel);
    process.removeListener('SIGTERM', cancel);
  }
  const inspected: Array<MutationWorkerResult & { workerIndex: number;
    artifactPath?: string; [key: string]: unknown }> = workers;
  const failures = inspected.flatMap(worker => worker.failures
    .map(failure => `worker ${worker.workerIndex + 1}: ${failure}`));
  if (cancellation.signal.aborted) failures.unshift(String(cancellationReason));
  let results: Array<{ status?: unknown; [key: string]: unknown }> = [];
  try {
    const merged = parallelMutationResults(manifest, inspected);
    if (Array.isArray(merged)) results = merged;
  } catch (error) { failures.push(errorMessage(error)); }
  const representative = clean;
  const caught = results.filter(result => result.status === 'CAUGHT').length;
  if (results.length !== manifest.mutations.length) {
    failures.push(`mutation results are incomplete: ${results.length}/${manifest.mutations.length}`);
  } else if (caught !== results.length) failures.push('one or more mutations were not cleanly caught');
  const fingerprints = new Set([clean.fingerprint].filter(Boolean));
  const images = new Set([clean.imageId, ...inspected.map(worker => worker.run?.imageId)]
    .filter(Boolean));
  const harnesses = new Set([clean.harnessSha256Before, clean.harnessSha256After,
    ...inspected.flatMap(worker =>
      [worker.run?.harnessSha256Before, worker.run?.harnessSha256After])].filter(Boolean));
  if (fingerprints.size !== 1) failures.push('clean baseline fingerprint is missing');
  if (images.size !== 1) failures.push('worker build images differ');
  if (harnesses.size !== 1) failures.push('worker harness identities differ');
  return { repetition: repetition + 1,
    output: relative(String(args.artifactDirectory), workerRoot).replaceAll('\\', '/'),
    durationMs: Date.now() - started,
    processError: failures.length ? failures.join('; ') : null,
    harnessSha256Before: harnesses.size === 1 ? [...harnesses][0] : null,
    harnessSha256After: harnesses.size === 1 ? [...harnesses][0] : null,
    ok: failures.length === 0,
    failures,
    runId: id,
    score: representative.score ?? null,
    imageId: images.size === 1 ? [...images][0] : null,
    criteria: representative.criteria ?? null,
    zeroPointCriteria: representative.zeroPointCriteria ?? null,
    fingerprint: fingerprints.size === 1 ? [...fingerprints][0] : null,
    outcome: failures.length === 0 ? 'passed' : 'harness_failure',
    packRuntime: representative.packRuntime ?? null,
    mutations: { caught, total: results.length },
    baselineDurationMs: clean.durationMs,
    baselineOutput: clean.output,
    baselineHarnessSha256Before: clean.harnessSha256Before,
    baselineHarnessSha256After: clean.harnessSha256After,
    workers: inspected.map(worker => ({ index: worker.workerIndex, runIndex: worker.runIndex,
      artifact: relative(String(args.artifactDirectory),
        String(worker.artifactPath)).replaceAll('\\', '/'),
      mutationIds: worker.assigned, ok: worker.failures.length === 0,
      logs: Object.fromEntries(Object.entries(
        (record(worker.processResult) ? worker.processResult.logs : null) ?? {})
        .map(([name, log]) => [name, relative(String(args.artifactDirectory),
          String(record(log) ? log.path : '')).replaceAll('\\', '/')])) })),
  };
}

export function referenceRunFromMutationBaseline(artifactDirectory: string,
  mutationRun: UnknownRecord, fixture: ReferenceFixture,
  { release, level, selectedCheckKeys }: {
    release?: UnknownRecord | null; level?: number; selectedCheckKeys?: string[] | null;
  }): UnknownRecord {
  const output = mutationRun.baselineOutput ?? mutationRun.output;
  const audit = auditReferenceRun(resolve(artifactDirectory, String(output)), fixture,
    { release, level, selectedCheckKeys });
  return {
    repetition: mutationRun.repetition,
    output,
    durationMs: mutationRun.baselineDurationMs ?? mutationRun.durationMs,
    processError: null,
    harnessSha256Before: mutationRun.baselineHarnessSha256Before
      ?? mutationRun.harnessSha256Before,
    harnessSha256After: mutationRun.baselineHarnessSha256After
      ?? mutationRun.harnessSha256After,
    ...audit,
  };
}

function finalizeQualificationArtifact(artifact: QualificationArtifact,
  { referenceMutationOnly = false }: { referenceMutationOnly?: boolean } = {}):
  QualificationArtifact {
  const complete = artifact.runs.length === artifact.requiredRepetitions;
  const fingerprints = new Set(artifact.runs.map(run => run.fingerprint).filter(Boolean));
  const images = new Set(artifact.runs.map(run => run.imageId).filter(Boolean));
  const harnessHashes = new Set(artifact.runs.flatMap(run =>
    [run.harnessSha256Before, run.harnessSha256After]).filter(Boolean));
  artifact.stable = referenceMutationOnly
    ? complete : complete && fingerprints.size === 1 && artifact.runs.every(run => run.fingerprint);
  artifact.sameImage = complete && images.size === 1 && artifact.runs.every(run => run.imageId);
  artifact.sameHarness = complete && harnessHashes.size === 1;
  artifact.harnessSha256 = artifact.sameHarness ? [...harnessHashes][0] : null;
  artifact.ok = Boolean(complete
    && artifact.runs.every(run => run.ok) && artifact.stable && artifact.sameImage
    && artifact.sameHarness);
  artifact.completedAt = new Date().toISOString();
  return artifact;
}

export function qualificationArtifactsOk(artifact: UnknownRecord,
  companion: UnknownRecord | null = null): boolean {
  return artifact?.ok === true && (companion === null || companion?.ok === true);
}

async function main(): Promise<void> {
  const args = parseReferenceQualificationArgs(process.argv);
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) throw new Error(`reference registry is invalid:\n${validation.issues.join('\n')}`);
  if (args.backend === undefined) throw new Error('reference qualification requires a backend');
  const selection = resolveReferenceSelection(registry,
    { ...args, backend: args.backend, track: args.track, level: args.level });
  const fixture = selection.fixture;
  const inspection = inspectImportedReference(fixture);
  if (!inspection.ok) throw new Error(`${fixture.id} import is invalid:\n${inspection.failures.join('\n')}`);
  const context = referenceQualificationContext(fixture, selection.recipe,
    { level: args.level, featureCatalog: args.featureCatalog });
  assertReleaseCandidateRepetitions(args, context.calibration);
  const selectedManifest = args.mutations
    ? qualificationMutationManifest(fixture, context, args.mutationIds) : null;
  const runContext = args.mutationIds.length && selectedManifest
    ? { ...context, selectedCheckKeys: targetedMutationCheckKeys(context, selectedManifest) }
    : context;

  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const id = `reference-live-${fixture.backend}-${stamp}-${process.pid}`;
  const paths = referenceQualificationPaths(args, id);
  if (existsSync(paths.artifactPath)) {
    throw new Error(`refusing to replace existing qualification artifact: ${paths.artifactPath}`);
  }
  if (existsSync(paths.runsRoot)) {
    throw new Error(`refusing to reuse existing qualification run directory: ${paths.runsRoot}`);
  }
  const companionPath = args.releaseCandidate && args.mutations
    && context.calibration.qualification.referenceRepetitions === args.repetitions
    ? companionReferenceArtifactPath(paths.artifactPath) : null;
  if (companionPath && existsSync(companionPath)) {
    throw new Error(`refusing to replace existing companion reference artifact: ${companionPath}`);
  }
  args.artifactDirectory = paths.artifactDirectory;
  args.runsRoot = paths.runsRoot;
  if (args.mutations) {
    args.mutationCheckpointDir ??= join(paths.artifactDirectory,
      `${basename(paths.artifactPath, extname(paths.artifactPath))}.mutation-checkpoints`);
    mkdirSync(String(args.mutationCheckpointDir), { recursive: true });
    if (args.mutationWorkers === 1 && !args.mutationCheckpoint) {
      args.mutationCheckpoint = join(String(args.mutationCheckpointDir),
        `${args.backend}-worker-1.json`);
    }
  }
  const selectedReference = context.calibration.references.entries.find(entry =>
    entry.backend === fixture.backend && entry.id === fixture.id);
  const selectedMutation = args.mutations && selectedManifest
    ? { ...context.calibration.mutations.find(entry => entry.backend === fixture.backend),
      backend: fixture.backend,
      executionSha256: mutationExecutionSha256(selectedManifest) }
    : null;
  const qualificationRelease = referenceQualificationRelease(runContext.binding.release,
    runContext.selectedCheckKeys ?? []);
  const qualificationScope = qualificationScopeIdentity({
    kind: args.mutations ? 'mutation' : 'reference',
    release: qualificationRelease,
    stack: fixture.backend,
    reference: selectedReference,
    mutation: selectedMutation,
    stackBenchRoot: ROOT,
  });
  const artifact: QualificationArtifact = {
    id, kind: 'reference_qualification', fixture: fixture.id,
    identities: emptyArtifactIdentities({
      fixture: { id: fixture.id, sha256: fixture.imported?.sourceSha256, state: fixture.status },
      recipe: { id: context.binding.release.id, version: context.binding.release.version,
        sha256: context.binding.release.contentSha256, state: context.binding.release.state },
      calibration: { ...(record(context.identity) ? context.identity : {}),
        state: context.calibration.state },
      stackAdapter: { id: fixture.backend },
    }),
    fixtureSha256: fixture.imported?.sourceSha256, requiredRepetitions: args.repetitions,
    startedAt: new Date().toISOString(), isolation: 'docker',
    runner: controllerRunner(), qualificationScope, mutationControl: args.mutations,
    diagnostic: args.mutationIds.length > 0,
    qualifiedCheckKeys: [...(runContext.selectedCheckKeys ?? [])].sort(),
    ...(runContext.featureCatalog ? { featureCatalog: runContext.featureCatalog } : {}), runs: [] };
  const companion: QualificationArtifact | null = companionPath ? {
    ...artifact,
    id: `${id}-reference`,
    qualificationScope: qualificationScopeIdentity({
      kind: 'reference', release: qualificationRelease, stack: fixture.backend,
      reference: selectedReference, stackBenchRoot: ROOT,
    }),
    mutationControl: false,
    diagnostic: false,
    runs: [],
  } : null;
  const artifactIdentities: UnknownRecord = record(artifact.identities)
    ? artifact.identities : {};
  for (let repetition = 0; repetition < args.repetitions; repetition++) {
    console.log(`\nqualifying ${fixture.id}: clean run ${repetition + 1}/${args.repetitions}`);
    let companionCaptured = false;
    const captureCompanion = (cleanRun: UnknownRecord): void => {
      if (!companion || companionCaptured) return;
      companion.runs.push(referenceRunFromMutationBaseline(String(args.artifactDirectory), cleanRun,
        fixture, { release: context.binding.release, level: args.level,
          selectedCheckKeys: runContext.selectedCheckKeys ?? null }));
      finalizeQualificationArtifact(companion);
      writeRunJson(String(companionPath), companion);
      companionCaptured = true;
    };
    const run = args.releaseCandidate || args.mutationWorkers > 1
      ? await runParallelMutationRepetition(fixture, args, runContext, id, repetition,
        artifactIdentities, captureCompanion)
      : await runOnce(fixture, args, runContext, id, repetition);
    artifact.runs.push(run);
    captureCompanion(run);
    // Repetition measures stability of a passing baseline. Repeating a setup or
    // infrastructure failure only wastes time and produces duplicate noise.
    if (!run.ok) break;
  }
  finalizeQualificationArtifact(artifact,
    { referenceMutationOnly: Boolean(args.referenceMutationOnly) });
  writeRunJson(paths.artifactPath, artifact);
  if (companion) {
    finalizeQualificationArtifact(companion);
    writeRunJson(String(companionPath), companion);
  }
  const ok = qualificationArtifactsOk(artifact, companion);
  console.log(JSON.stringify({ ok, artifact: paths.artifactPath, stable: artifact.stable,
    sameImage: artifact.sameImage, sameHarness: artifact.sameHarness, diagnostic: artifact.diagnostic,
    ...(companion ? { referenceArtifact: companionPath, referenceOk: companion.ok } : {}),
    runs: artifact.runs.map(({ repetition, ok, score, criteria,
      zeroPointCriteria, mutations, failures }) => ({ repetition, ok, score, criteria, zeroPointCriteria,
      mutations, failures })) }, null, 2));
  if (!ok) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 2; });
}
