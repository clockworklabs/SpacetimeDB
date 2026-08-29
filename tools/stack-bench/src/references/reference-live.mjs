#!/usr/bin/env node
// Repeatable live qualification for an imported reference fixture.
//
// A reference is promotable only after independent, clean Docker runs agree
// and every criterion passes, including zero-point controls. bench.mjs remains
// the lifecycle owner; this wrapper supplies fresh copies and audits the
// resulting evidence instead of trusting the benchmark process exit code.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync,
  writeFileSync, writeSync } from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { executeStackCapability } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeRunJson } from '../evidence/artifacts.js';
import { hashDirectory } from '../evidence/provenance.js';
import { inspectImportedReference, loadReferenceRegistry, prepareReferenceFixtureSource,
  validateReferenceRegistry } from './reference-fixtures.mjs';
import { resolveReferenceSelection } from './reference-selection.js';
import { killTree } from '../runtime/platform.js';
import { criterionEvidence, evidencePassed } from '../evidence/check-evidence.js';
import { recoverSupervisedRun, validateSupervisorState } from '../runtime/recovery.js';
import { calibrationQualificationIdentity, mutationExecutionSha256,
  resolveCalibrationForRelease } from '../composition/calibration-compiler.mjs';
import { qualificationScopeIdentity } from '../composition/qualification-scope.js';
import { resolveRecipeRelease } from '../composition/recipe-release.mjs';
import { isModularRecipeRelease } from '../composition/recipe-selection.mjs';
import { isDeclaredLevel, listTracks, loadTrack } from '../composition/tracks.mjs';
import { RUN_INDEX_CAP } from '../composition/tracks.mjs';
import { controllerRunner } from '../runtime/runner-environment.js';
import { mergeMutationShards, mutationShard, mutationWorkerSlots }
  from '../evidence/mutation-shards.js';
import { existingResourceLockKeys, resourceLockScope } from '../runtime/backend-lease.mjs';
import { resolveFeatureCatalog } from '../progression/feature-catalog-selection.js';
import { progressionLevels, selectFeatureCatalogLevels }
  from '../progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../progression/progression-recipe-selection.js';
import { mutationTargetKeys } from '../evidence/mutation-analysis.js';

export { controllerRunner as referenceQualificationRunner } from '../runtime/runner-environment.js';

import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';
const BENCH = join(ROOT, 'commands', 'bench.mjs');
const DEFAULT_SPACETIME_PORT = 3310;

function qualificationInputs() {
  const ignoredRoots = new Set(['archive', 'results', 'node_modules', 'reference-apps']);
  return hashDirectory(ROOT, { exclude: (name, entry) => {
    const parts = name.split('/');
    if (ignoredRoots.has(parts[0]) || parts.some(part => part.startsWith('.spacetime-data'))) return true;
    if (entry.isDirectory()) return false;
    return !(/\.(?:mjs|js|json|ya?ml|sh)$/.test(name) || /(?:^|\/)Dockerfile$/.test(name));
  } });
}

export function parseReferenceQualificationArgs(argv) {
  const args = { track: 'ecommerce', level: 1, repetitions: 2,
    runIndex: 0, spacetimePort: null, timeoutMinutes: null, mutations: false,
    mutationWorkers: 1, mutationShardIndex: null, mutationShardCount: null,
    mutationMaxRuntimeMinutes: 60, mutationIds: [] };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--backend') args.backend = argv[++i];
    else if (argv[i] === '--track') args.track = argv[++i];
    else if (argv[i] === '--level') args.level = Number(argv[++i]);
    else if (argv[i] === '--recipe') args.recipe = argv[++i];
    else if (argv[i] === '--feature-catalog') args.featureCatalog = argv[++i];
    else if (argv[i] === '--repetitions') args.repetitions = Number(argv[++i]);
    else if (argv[i] === '--run-index') args.runIndex = Number(argv[++i]);
    else if (argv[i] === '--spacetime-port') {
      args.spacetimePort = Number(argv[++i]);
      args.spacetimePortExplicit = true;
    }
    else if (argv[i] === '--timeout-minutes') args.timeoutMinutes = Number(argv[++i]);
    else if (argv[i] === '--mutations') args.mutations = true;
    else if (argv[i] === '--release-candidate') args.releaseCandidate = true;
    else if (argv[i] === '--mutation-id') args.mutationIds.push(argv[++i]);
    else if (argv[i] === '--mutation-workers') args.mutationWorkers = Number(argv[++i]);
    else if (argv[i] === '--mutation-shard-index') args.mutationShardIndex = Number(argv[++i]);
    else if (argv[i] === '--mutation-shard-count') args.mutationShardCount = Number(argv[++i]);
    else if (argv[i] === '--mutation-checkpoint-dir') args.mutationCheckpointDir = resolve(argv[++i]);
    else if (argv[i] === '--mutation-checkpoint') args.mutationCheckpoint = resolve(argv[++i]);
    else if (argv[i] === '--mutation-baseline-bundle') args.mutationBaselineBundle = resolve(argv[++i]);
    else if (argv[i] === '--mutation-max-runtime-minutes') {
      args.mutationMaxRuntimeMinutes = Number(argv[++i]);
    }
    else if (argv[i] === '--reference-mutation-only') args.referenceMutationOnly = true;
    else if (argv[i] === '--out') args.out = resolve(argv[++i]);
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  if (!STACK_ADAPTER_REGISTRY.ids.includes(args.backend)) {
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
    mutationWorkerSlots({ workerCount: args.mutationShardCount, runIndex: 0,
      maxRunIndex: RUN_INDEX_CAP });
    if (!Number.isInteger(args.mutationShardIndex) || args.mutationShardIndex < 0
        || args.mutationShardIndex >= args.mutationShardCount) {
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

export function runBounded(command, argv,
  { cwd, env, stdio = 'inherit', timeoutMs, terminate = killTree, logs = null, signal = null,
    gracefulCancellationMs = 0 }) {
  return new Promise(resolveRun => {
    const maximum = logs?.maxBytes ?? 4 * 1024 * 1024;
    if (logs && (!Number.isInteger(maximum) || maximum <= 0)) {
      throw new Error('runBounded logs.maxBytes must be a positive integer');
    }
    if (logs) {
      for (const name of ['stdout', 'stderr']) {
        if (typeof logs[name] !== 'string' || !logs[name]) {
          throw new Error(`runBounded logs.${name} must be a path`);
        }
      }
      if (resolve(logs.stdout) === resolve(logs.stderr)) {
        throw new Error('runBounded stdout and stderr logs must use different paths');
      }
    }
    const streams = logs ? Object.fromEntries(['stdout', 'stderr'].map(name => {
      const path = resolve(logs[name]);
      mkdirSync(dirname(path), { recursive: true });
      return [name, { path, fd: openSync(path, 'w', 0o600), bytes: 0, retainedBytes: 0,
        hash: createHash('sha256'), tail: '' }];
    })) : null;
    const child = spawn(command, argv, { cwd, env,
      stdio: streams ? ['inherit', 'pipe', 'pipe'] : stdio });
    const capture = (name, destination) => chunk => {
      const state = streams[name];
      const data = Buffer.from(chunk);
      state.bytes += data.length;
      const remaining = maximum - state.retainedBytes;
      if (remaining > 0) {
        const retained = data.subarray(0, remaining);
        writeSync(state.fd, retained);
        state.hash.update(retained);
        state.retainedBytes += retained.length;
      }
      state.tail = `${state.tail}${data.toString('utf8')}`.slice(-2000);
      destination.write(data);
    };
    if (streams) {
      child.stdout.on('data', capture('stdout', process.stdout));
      child.stderr.on('data', capture('stderr', process.stderr));
    }
    let timedOut = false;
    let cancelled = false;
    let spawnError = null;
    let forceTimer = null;
    const stop = () => {
      terminate(child.pid);
      try { child.kill('SIGKILL'); } catch { /* already exited */ }
    };
    const cancel = () => {
      cancelled = true;
      if (gracefulCancellationMs > 0) {
        try { child.kill('SIGTERM'); } catch { /* already exited */ }
        forceTimer = setTimeout(stop, gracefulCancellationMs);
        forceTimer.unref();
      } else stop();
    };
    if (signal) {
      if (signal.aborted) cancel();
      else signal.addEventListener('abort', cancel, { once: true });
    }
    const timer = setTimeout(() => {
      timedOut = true;
      // SIGTERM handlers are bypassed by Node on Windows. Kill the exact child
      // tree, then let the lease-aware supervisor remove detached resources.
      stop();
    }, timeoutMs);
    child.once('error', error => { spawnError = error; });
    child.once('close', (code, childSignal) => {
      clearTimeout(timer);
      if (forceTimer) clearTimeout(forceTimer);
      signal?.removeEventListener('abort', cancel);
      const captured = streams ? Object.fromEntries(Object.entries(streams).map(([name, state]) => {
        closeSync(state.fd);
        return [name, { path: state.path, sha256: state.hash.digest('hex'), bytes: state.bytes,
          retainedBytes: state.retainedBytes, truncated: state.bytes > state.retainedBytes }];
      })) : null;
      resolveRun({ ok: !timedOut && !cancelled && !spawnError && code === 0,
        code, signal: childSignal, timedOut, cancelled,
        error: spawnError, logs: captured,
        stdoutTail: streams?.stdout.tail.trim() || '', stderrTail: streams?.stderr.tail.trim() || '' });
    });
  });
}

export function rescueSupervisedLease(path, output) {
  if (!existsSync(path)) return;
  const state = validateSupervisorState(readJson(path), { source: path });
  if (resolve(output) !== resolve(state.output)) {
    throw new Error(`supervisor output does not match requested output: ${state.runId}`);
  }
  if (!existsSync(state.leasePath)) {
    const runPath = join(output, 'run.json');
    if (!existsSync(runPath)) throw new Error(`backend lease disappeared without released run evidence: ${state.runId}`);
    const runArtifact = readArtifact(runPath);
    if (!['benchmark_run', 'repair_continuation'].includes(runArtifact.kind)) {
      throw new Error(`backend lease disappeared with unexpected run artifact ${runArtifact.kind}`);
    }
    const lease = runArtifact.payload.backendLease;
    const released = lease?.runId === state.runId && lease?.state === 'released'
      && lease?.resources?.buildContainer?.running === false
      && lease?.resources?.locks?.length > 0
      && lease.resources.locks.every(lock => Boolean(lock.releasedAt));
    if (!released) throw new Error(`backend lease disappeared without released run evidence: ${state.runId}`);
    return;
  }
  const result = recoverSupervisedRun(path, { removeState: false });
  if (!result.ok) throw new Error(`supervisor could not release backend lease ${state.runId}`);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function auditReferenceRun(output, fixture,
  { requireMutationControl = false, release = null, level = fixture.level,
    selectedCheckKeys = null } = {}) {
  const runPath = join(output, 'run.json');
  const bundlePath = join(output, 'grading', 'bundle.json');
  if (!existsSync(runPath) || !existsSync(bundlePath)) {
    return { ok: false, failures: ['run.json or grading/bundle.json is missing'] };
  }
  const run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
  const bundle = readArtifactPayload(bundlePath, { expectedKind: 'grade_bundle' });
  const failures = [];
  if (!release?.contentSha256 || !Array.isArray(release.checkCatalog)
      || release.checkCatalog.length === 0) {
    failures.push('exact recipe release was not supplied to the qualification audit');
  }
  if (run.backend !== fixture.backend || run.track !== fixture.track) failures.push('run identity does not match fixture');
  if (run.setup?.isolation?.mode !== 'container') failures.push('run was not isolated in Docker');
  if (run.outcome?.kind !== 'passed') failures.push(`run outcome is ${run.outcome?.kind ?? 'missing'}`);
  const levelResult = run.levels?.find(candidate => candidate.level === level);
  if (!levelResult) failures.push(`L${level} result is missing`);
  else {
    if (!levelResult.graded) failures.push(`L${level} was not graded`);
    if (!levelResult.contractPass) failures.push(`L${level} contract lint failed`);
    if (levelResult.score !== levelResult.max) {
      failures.push(`L${level} score is ${levelResult.score}/${levelResult.max}`);
    }
  }

  const criteria = [];
  for (const [suiteId, suite] of Object.entries(bundle.suites ?? {})) {
    if (suiteId === 'lint') continue;
    for (const feature of suite.features ?? []) {
      const setupFailure = (feature.criteria ?? []).map(criterionEvidence)
        .find(evidence => evidence.phase === 'setup' && !evidencePassed(evidence));
      if (setupFailure) failures.push(`${suiteId}/${feature.id} setup failed: ${setupFailure.summary}`);
      for (const criterion of feature.criteria ?? []) {
        const key = `${suiteId}/${feature.id}/${criterion.id}`;
        const evidence = criterionEvidence(criterion);
        criteria.push({ key, stableKey: criterion.stableKey ?? null,
          points: criterion.points ?? 0, passed: evidencePassed(evidence), status: evidence.status });
        if (!evidencePassed(evidence)) failures.push(`${key} did not pass`);
      }
    }
  }
  if (!criteria.length) failures.push('no scenario criteria were recorded');
  if (release) {
    const expectedRecipe = { id: release.id, version: release.version,
      contentSha256: release.contentSha256 };
    const actualRecipe = bundle.selection?.recipe;
    if (actualRecipe?.id !== expectedRecipe.id || actualRecipe?.version !== expectedRecipe.version
        || actualRecipe?.contentSha256 !== expectedRecipe.contentSha256) {
      failures.push('grading bundle recipe identity does not match the requested release');
    }
    if (bundle.recipeRelease?.contentSha256 !== release.contentSha256) {
      failures.push('grading bundle release document does not match the requested release');
    }
    const fields = ['stableKey', 'points', 'source', 'executionId', 'featureId',
      'criterionId', 'packId', 'checkGroupId'];
    const shape = check => Object.fromEntries(fields.map(field => [field, check?.[field] ?? null]));
    const order = checks => checks.map(shape).sort((a, b) => a.stableKey.localeCompare(b.stableKey));
    const selected = selectedCheckKeys === null ? release.checkCatalog : (() => {
      const requested = new Set(selectedCheckKeys);
      const checks = release.checkCatalog.filter(check => requested.delete(check.stableKey));
      if (requested.size) failures.push(`selected checks are absent from the recipe: ${
        [...requested].sort().join(', ')}`);
      return checks;
    })();
    const expectedChecks = order(selected);
    const selectedChecks = Array.isArray(bundle.selection?.checks)
      ? order(bundle.selection.checks) : null;
    if (!selectedChecks || JSON.stringify(selectedChecks) !== JSON.stringify(expectedChecks)) {
      failures.push('graded check catalog does not match the requested release');
    }
    const expectedKeys = expectedChecks.map(check => check.stableKey);
    const reportedKeys = [...(bundle.selection?.reportedChecks ?? [])].sort();
    const evidenceKeys = criteria.map(check => check.stableKey).sort();
    if (JSON.stringify(reportedKeys) !== JSON.stringify(expectedKeys)
        || JSON.stringify(evidenceKeys) !== JSON.stringify(expectedKeys)) {
      failures.push('graded check evidence does not cover the exact requested release');
    }
  }
  const lease = run.backendLease;
  if (lease?.state !== 'released') failures.push('backend lease was not released');
  if (lease?.resources?.buildContainer?.running !== false) failures.push('leased build container was not recorded as removed');
  if (!lease?.resources?.locks?.length) failures.push('run recorded no resource lock');
  for (const lock of lease?.resources?.locks ?? []) {
    if (!lock.releasedAt) failures.push(`resource lock ${lock.key ?? lock.path} was not released`);
  }
  let mutationControl = null;
  if (requireMutationControl) {
    const mutationPath = join(output, 'mutation-control.json');
    if (!existsSync(mutationPath)) failures.push('mutation-control.json is missing');
    else {
      mutationControl = readArtifactPayload(mutationPath, { expectedKind: 'mutation_control' });
      if (run.mutationControl?.ok !== true || mutationControl.ok !== true) {
        failures.push('mutation control did not pass');
      }
      if (mutationControl.fixtureSha256 !== fixture.imported.sourceSha256) {
        failures.push('mutation control targets a different fixture hash');
      }
      if (!Array.isArray(mutationControl.results) || mutationControl.results.length === 0) {
        failures.push('mutation control recorded no mutants');
      } else {
        for (const mutant of mutationControl.results) {
          if (mutant.status !== 'CAUGHT') failures.push(`${mutant.id ?? '<unnamed mutant>'} is ${mutant.status ?? 'missing a status'}`);
        }
      }
      if (Number(mutationControl.baseline?.total) !== Number(mutationControl.baseline?.max)) {
        failures.push('mutation baseline was not fully passing');
      }
    }
  }
  const fingerprint = createHash('sha256').update(JSON.stringify(criteria)).digest('hex');
  return { ok: failures.length === 0, failures, runId: run.id,
    score: levelResult ? `${levelResult.score}/${levelResult.max}` : null,
    imageId: run.setup?.isolation?.imageId ?? null, criteria: criteria.length,
    zeroPointCriteria: criteria.filter(criterion => criterion.points === 0).length, fingerprint,
    outcome: run.outcome?.kind ?? null, packRuntime: bundle.packRuntime ?? null,
    mutations: mutationControl?.summary ?? null };
}

export function auditMutationWorkerRun(output, fixture) {
  const runPath = join(output, 'run.json');
  const controlPath = join(output, 'mutation-control.json');
  if (!existsSync(runPath) || !existsSync(controlPath)) {
    return { ok: false, failures: ['run.json or mutation-control.json is missing'] };
  }
  const run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
  const control = readArtifactPayload(controlPath, { expectedKind: 'mutation_control' });
  const failures = [];
  if (run.backend !== fixture.backend || run.track !== fixture.track) {
    failures.push('run identity does not match fixture');
  }
  if (run.setup?.isolation?.mode !== 'container') failures.push('run was not isolated in Docker');
  if (run.outcome?.kind !== 'passed') failures.push(`run outcome is ${run.outcome?.kind ?? 'missing'}`);
  if (run.mutationControl?.ok !== true || control.ok !== true) {
    failures.push('mutation control did not pass');
  }
  if (control.fixtureSha256 !== fixture.imported.sourceSha256) {
    failures.push('mutation control targets a different fixture hash');
  }
  if (!Array.isArray(control.results) || control.results.length === 0) {
    failures.push('mutation control recorded no mutants');
  } else {
    for (const mutant of control.results) {
      if (mutant.status !== 'CAUGHT') {
        failures.push(`${mutant.id ?? '<unnamed mutant>'} is ${mutant.status ?? 'missing a status'}`);
      }
    }
  }
  if (Number(control.baseline?.total) !== Number(control.baseline?.max)) {
    failures.push('mutation baseline was not fully passing');
  }
  const lease = run.backendLease;
  if (lease?.state !== 'released') failures.push('backend lease was not released');
  if (lease?.resources?.buildContainer?.running !== false) {
    failures.push('leased build container was not recorded as removed');
  }
  if (!lease?.resources?.locks?.length
      || lease.resources.locks.some(lock => !lock.releasedAt)) {
    failures.push('resource lock release evidence is incomplete');
  }
  return { ok: failures.length === 0, failures, runId: run.id,
    imageId: run.setup?.isolation?.imageId ?? null, outcome: run.outcome?.kind ?? null,
    mutations: control.summary ?? null, score: null, criteria: null,
    zeroPointCriteria: null, fingerprint: null, packRuntime: null };
}

export function referenceQualificationContext(fixture, recipe = null,
  { level = fixture.level, featureCatalog = null } = {}) {
  const track = loadTrack(fixture.track);
  const binding = resolveRecipeRelease(track, level, recipe);
  if (!binding) throw new Error(`${fixture.track} L${level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, stackBenchRoot: ROOT, alias: `L${level}` });
  if (!calibration) {
    throw new Error(`${binding.release.id}@${binding.release.version} has no L${level} calibration`);
  }
  const reference = calibration.references.entries.find(entry => entry.backend === fixture.backend
    && entry.id === fixture.id && entry.sourceSha256 === fixture.imported.sourceSha256);
  if (!reference) throw new Error(`${fixture.id} is not selected by calibration ${calibration.id}`);
  const declaredCatalog = calibration.qualification.featureCatalog;
  const catalogRef = featureCatalog ?? (declaredCatalog
    ? `${declaredCatalog.id}@${declaredCatalog.version}` : null);
  const fullCatalog = catalogRef ? resolveFeatureCatalog(catalogRef, track) : null;
  const catalog = fullCatalog ? selectFeatureCatalogLevels(fullCatalog,
    progressionLevels(fullCatalog).filter(candidate => candidate <= level)) : null;
  if (declaredCatalog && catalog.identity.sha256 !== declaredCatalog.sha256) {
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

export function referenceQualificationSelectionArgs(binding, progressionSelection = null,
  selectedCheckKeys = null) {
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
  const taskMode = progressionSelection?.grader.request.task.mode;
  if (progressionSelection && !['fresh', 'upgrade'].includes(taskMode)) {
    throw new Error('progression reference qualification requires a fresh or upgrade task mode');
  }
  return ['--feature-module', features.join(','), '--expect-spec', specifications.join(','),
    ...(taskMode ? ['--task-mode', taskMode] : []), ...args];
}

export function referenceQualificationRelease(release, selectedCheckKeys) {
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

export function referenceQualificationPaths(args, id) {
  const artifactPath = args.out ?? join(ROOT, 'results', 'reference-live', `${id}.json`);
  const artifactName = basename(artifactPath, extname(artifactPath));
  return {
    artifactPath,
    artifactDirectory: dirname(artifactPath),
    runsRoot: join(dirname(artifactPath), `${artifactName}.runs`),
  };
}

export function companionReferenceArtifactPath(mutationArtifactPath) {
  const extension = extname(mutationArtifactPath) || '.json';
  const stem = basename(mutationArtifactPath, extension);
  const referenceStem = stem.endsWith('-mutation')
    ? `${stem.slice(0, -'-mutation'.length)}-reference`
    : `${stem}-reference`;
  return join(dirname(mutationArtifactPath), `${referenceStem}${extension}`);
}

export function assertReleaseCandidateRepetitions(args, calibration) {
  if (!args.releaseCandidate) return;
  const required = calibration?.qualification?.mutationRepetitions;
  if (!Number.isInteger(required) || required < 1) {
    throw new Error('release calibration has no valid mutation repetition count');
  }
  if (args.repetitions !== required) {
    throw new Error(`--release-candidate requires exactly ${required} mutation repetition(s)`);
  }
}

export function referenceQualificationWorkRoot(env = process.env) {
  return resolve(env.STACK_BENCH_WORK_DIR ?? tmpdir());
}

export function qualificationMutationManifest(fixture, context, requestedIds = []) {
  if (fixture.mutationManifests.length !== 1) {
    throw new Error(`${fixture.id} must own exactly one mutation manifest for live mutation qualification`);
  }
  const manifest = readJson(join(ROOT, fixture.mutationManifests[0]));
  const selection = context.calibration.mutations.find(entry => entry.backend === fixture.backend);
  if (!selection) throw new Error(`${fixture.id} has no mutation selection in its calibration`);
  const selectedIds = new Set(selection.targets.map(target => target.id));
  const mutations = manifest.mutations.filter(mutation => selectedIds.delete(mutation.id));
  if (selectedIds.size) {
    throw new Error(`${fixture.id} mutation selection is missing: ${[...selectedIds].sort().join(', ')}`);
  }
  if (mutations.length === 0) throw new Error(`${fixture.id} mutation selection is empty`);
  if (requestedIds.length === 0) return { ...manifest, mutations };
  const requested = new Set(requestedIds);
  const targeted = mutations.filter(mutation => requested.delete(mutation.id));
  if (requested.size) {
    throw new Error(`${fixture.id} targeted mutation selection is missing: ${[...requested].sort().join(', ')}`);
  }
  return { ...manifest, mutations: targeted };
}

export function targetedMutationCheckKeys(context, manifest) {
  const available = new Map(context.binding.release.checkCatalog.map(check =>
    [check.stableKey, check]));
  const allowed = new Set(context.selectedCheckKeys);
  const requested = new Set(manifest.mutations.flatMap(mutationTargetKeys));
  const missing = [...requested].filter(key => !available.has(key));
  if (missing.length) {
    throw new Error(`targeted mutations name unknown checks: ${missing.sort().join(', ')}`);
  }
  const outsideScope = [...requested].filter(key => !allowed.has(key)
    && Number(available.get(key).points) > 0);
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

async function runOnce(fixture, args, context, id, repetition) {
  const workRoot = referenceQualificationWorkRoot();
  mkdirSync(workRoot, { recursive: true });
  const work = mkdtempSync(join(workRoot, `reference-live-${fixture.backend}-`));
  const app = join(work, 'app');
  const output = join(args.runsRoot, `r${repetition + 1}`);
  const supervisorState = join(work, 'supervisor-state.json');
  const started = Date.now();
  const harnessBefore = qualificationInputs();
  const cancellation = new AbortController();
  const cancel = () => cancellation.abort();
  process.once('SIGINT', cancel);
  process.once('SIGTERM', cancel);
  let processError = null;
  let cleanupPending = false;
  try {
    prepareReferenceFixtureSource(fixture, app);
    const adapter = STACK_ADAPTER_REGISTRY.get(fixture.backend);
    const env = { ...process.env, STACK_BENCH_SUPERVISOR_STATE: supervisorState,
      ...executeStackCapability(adapter, 'run-policy', 'supervisor-env',
        { spacetimePort: args.spacetimePort }) };
    const benchArgs = [BENCH, '--backend', fixture.backend, '--track', fixture.track,
      '--levels', String(args.level), '--run-index', String(args.runIndex), '--fix-rounds', '0',
      '--app', app, '--out', output, '--agent-adapter', 'reference-fixture', '--skip-probe', '--no-media'];
    benchArgs.push('--recipe', `${context.binding.release.id}@${context.binding.release.version}`);
    benchArgs.push(...referenceQualificationSelectionArgs(context.binding,
      context.progressionSelection, context.selectedCheckKeys));
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
        if (existsSync(args.mutationCheckpoint)) {
          benchArgs.push('--mutation-resume-from', args.mutationCheckpoint);
        }
        benchArgs.push('--mutation-checkpoint-out', args.mutationCheckpoint);
      }
      benchArgs.push('--mutation-max-runtime-minutes', String(args.mutationMaxRuntimeMinutes));
      benchArgs.push('--expected-mutation-calibration-json', JSON.stringify({
        id: context.calibration.id,
        version: context.calibration.version,
        sha256: context.calibration.contentSha256,
        state: context.calibration.state,
      }));
      if (args.mutationBaselineBundle) {
        benchArgs.push('--mutation-baseline-bundle', args.mutationBaselineBundle);
      }
      if (args.referenceMutationOnly) benchArgs.push('--reference-mutation-only');
    }
    const child = await runBounded(process.execPath, benchArgs,
      { cwd: ROOT, stdio: 'inherit', env, timeoutMs: args.timeoutMs,
        signal: cancellation.signal });
    if (!child.ok) {
      const reason = child.timedOut
        ? `benchmark exceeded ${args.timeoutMinutes} minute repetition deadline`
        : child.cancelled ? 'benchmark was interrupted'
        : child.error?.message ?? `benchmark exited ${child.code ?? child.signal ?? 'without status'}`;
      try { rescueSupervisedLease(supervisorState, output); }
      catch (cleanupError) {
        cleanupPending = true;
        throw new Error(`${reason}; lease cleanup failed: ${cleanupError.message}; `
          + `recovery authority retained at ${supervisorState}`);
      }
      throw new Error(reason);
    }
  } catch (error) {
    processError = `${error.message}`;
  } finally {
    process.removeListener('SIGINT', cancel);
    process.removeListener('SIGTERM', cancel);
    // This wrapper created exactly this mkdtemp directory. bench.mjs owns and
    // removes its leased container before returning; deleting the caller-owned
    // source copy here cannot target another run.
    try {
      if (!cleanupPending) {
        rmSync(work, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
      }
    } catch (error) {
      // Cleanup evidence must not replace the benchmark failure that caused it.
      // Retain the exact owned path for diagnosis; a later run has a distinct
      // mkdtemp root and cannot accidentally adopt it.
      const cleanup = `owned temp cleanup failed (${error.code ?? 'unknown'}): ${work}`;
      processError = processError ? `${processError}; ${cleanup}` : cleanup;
    }
  }
  const audit = args.referenceMutationOnly
    ? auditMutationWorkerRun(output, fixture)
    : auditReferenceRun(output, fixture, {
      requireMutationControl: args.mutations,
      release: context.binding.release,
      level: args.level,
      selectedCheckKeys: context.selectedCheckKeys,
    });
  const harnessAfter = qualificationInputs();
  if (harnessAfter.sha256 !== harnessBefore.sha256) {
    audit.failures.unshift('qualification harness changed while this repetition was running');
  }
  if (processError) audit.failures.unshift(`benchmark process failed: ${processError}`);
  audit.ok = audit.failures.length === 0;
  return { repetition: repetition + 1,
    output: relative(args.artifactDirectory, output).replaceAll('\\', '/'),
    durationMs: Date.now() - started, processError,
    harnessSha256Before: harnessBefore.sha256, harnessSha256After: harnessAfter.sha256, ...audit };
}

export function parallelMutationChildArgv(args, context,
  { artifactPath, baselineBundle, repetition, workerIndex, workerCount }) {
  const runIndex = args.runIndex + workerIndex;
  const argv = [fileURLToPath(import.meta.url), '--backend', args.backend,
    '--track', args.track, '--level', String(args.level), '--recipe',
    `${context.binding.release.id}@${context.binding.release.version}`,
    '--repetitions', '1', '--run-index', String(runIndex), '--timeout-minutes',
    String(args.timeoutMinutes), '--mutations', '--mutation-shard-index',
    String(workerIndex), '--mutation-shard-count', String(workerCount), '--out', artifactPath];
  argv.push('--reference-mutation-only');
  argv.push('--mutation-max-runtime-minutes', String(args.mutationMaxRuntimeMinutes));
  argv.push('--mutation-baseline-bundle', baselineBundle);
  if (args.mutationCheckpointDir) {
    argv.push('--mutation-checkpoint', join(args.mutationCheckpointDir,
      `${args.backend}-worker-${workerIndex + 1}.json`));
  }
  if (context.featureCatalogRef) argv.push('--feature-catalog', context.featureCatalogRef);
  for (const mutationId of args.mutationIds) argv.push('--mutation-id', mutationId);
  if (args.spacetimePortExplicit) {
    argv.push('--spacetime-port', String(args.spacetimePort + workerIndex));
  }
  return argv;
}

export function parallelMutationResourceLockKeys(args) {
  const slots = mutationWorkerSlots({ workerCount: args.mutationWorkers,
    runIndex: args.runIndex, maxRunIndex: RUN_INDEX_CAP });
  const keys = slots.map(runIndex => `slot:${args.track}:${args.backend}:run${runIndex}`);
  if (args.backend === 'spacetime') {
    keys.push(...slots.map((_, workerIndex) =>
      `listener:http://127.0.0.1:${args.spacetimePort + workerIndex}`));
  }
  return keys.sort();
}

export function preflightParallelMutationResources(args, env = process.env) {
  const occupied = existingResourceLockKeys({
    root: resourceLockScope(env).root,
    keys: parallelMutationResourceLockKeys(args),
  });
  if (occupied.length) {
    throw new Error(`parallel mutation resources are already leased: ${occupied.join(', ')}`);
  }
}

function identityKey(identity) {
  return JSON.stringify({ id: identity?.id ?? null, version: identity?.version ?? null,
    sha256: identity?.sha256 ?? null, state: identity?.state ?? null });
}

export function readParallelMutationWorker(path, processResult, expected, manifest) {
  const failures = [];
  const assigned = mutationShard(manifest.mutations, { index: expected.workerIndex,
    count: expected.workerCount, defaultScenario: manifest.scenario }).mutationIds;
  if (!processResult.ok) {
    failures.push(processResult.timedOut ? 'worker timed out'
      : processResult.error?.message ?? `worker exited ${processResult.code ?? processResult.signal}`);
  }
  if (!existsSync(path)) return { artifact: null, payload: null, run: null, control: null,
    assigned, shardVerified: false, failures: [...failures, 'worker artifact is missing'] };
  let artifact;
  try { artifact = readArtifact(path); }
  catch (error) { return { artifact: null, payload: null, run: null, control: null,
    assigned, shardVerified: false,
    failures: [...failures, `worker artifact is invalid: ${error.message}`] }; }
  if (artifact.kind !== 'reference_qualification') failures.push('worker artifact has the wrong kind');
  const payload = artifact.payload;
  if (payload?.mutationControl !== true || payload?.requiredRepetitions !== 1
      || payload?.runs?.length !== 1) {
    failures.push('worker qualification shape is invalid');
  }
  for (const key of ['engine', 'fixture', 'recipe', 'calibration', 'stackAdapter']) {
    if (identityKey(artifact.identities?.[key]) !== identityKey(expected[key])) {
      failures.push(`worker ${key} identity does not match the parent`);
    }
  }
  const run = payload?.runs?.[0] ?? null;
  if (payload?.ok !== true || run?.ok !== true) failures.push('worker qualification did not pass');
  let control = null;
  if (run?.output) {
    const childRoot = resolve(dirname(path));
    const outputRoot = resolve(childRoot, run.output);
    if (outputRoot !== childRoot && !outputRoot.startsWith(`${childRoot}${sep}`)) {
      failures.push('worker run output escapes its artifact directory');
    } else {
      const controlPath = join(outputRoot, 'mutation-control.json');
      try { control = readArtifactPayload(controlPath, { expectedKind: 'mutation_control' }); }
      catch (error) { failures.push(`worker mutation artifact is invalid: ${error.message}`); }
    }
  } else failures.push('worker run output is missing');
  const shardVerified = control?.shard?.index === expected.workerIndex
    && control.shard.count === expected.workerCount
    && JSON.stringify(control.shard.mutationIds) === JSON.stringify(assigned);
  if (control?.shard && !shardVerified) {
    failures.push('worker mutation shard does not match its assignment');
  }
  if (control?.results && !control.shard) {
    failures.push('worker mutation results do not bind their shard assignment');
  }
  if (control?.ok !== true) failures.push('worker mutation control did not pass');
  return { artifact, payload, run, control, assigned, shardVerified, failures };
}

export function parallelMutationResults(manifest, workers) {
  const completed = workers.filter(worker =>
    worker.shardVerified && Array.isArray(worker.control?.results));
  if (completed.length !== workers.length) {
    const byId = new Map(completed.flatMap(worker => worker.control.results)
      .map(result => [result.id, result]));
    return manifest.mutations.map(mutation => byId.get(mutation.id)).filter(Boolean);
  }
  return mergeMutationShards(manifest.mutations, completed.map(worker => ({
    index: worker.control.shard.index,
    count: worker.control.shard.count,
    mutationIds: worker.control.shard.mutationIds,
    results: worker.control.results,
  })), { defaultScenario: manifest.scenario });
}

export function mutationWorkerRequiresSiblingAbort(processResult, worker) {
  if (worker.control?.outcome?.kind === 'harness_failure') return true;
  if (processResult.ok) return false;
  const completed = worker.control?.checkpoint?.status === 'complete'
    && Array.isArray(worker.control?.results)
    && worker.control.results.length === worker.assigned.length;
  return !completed;
}

async function runParallelMutationRepetition(fixture, args, context, id, repetition,
  artifactIdentities, onCleanBaseline = null) {
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
      failures: clean.failures.map(failure => `clean baseline: ${failure}`),
      mutations: { caught: 0, total: 0 } };
  }
  const baselineBundle = resolve(args.artifactDirectory, clean.output,
    `first-build-l${args.level}-grading`, 'bundle.json');
  if (!existsSync(baselineBundle)) {
    return { ...clean, ok: false, durationMs: Date.now() - started,
      processError: `clean baseline bundle is missing: ${baselineBundle}`,
      failures: [`clean baseline bundle is missing: ${baselineBundle}`],
      outcome: 'incomplete', mutations: { caught: 0, total: 0 } };
  }
  if (onCleanBaseline) onCleanBaseline(clean);
  const remainingMs = started + args.timeoutMs - Date.now();
  if (remainingMs <= 0) {
    return { ...clean, ok: false, durationMs: Date.now() - started,
      processError: 'mutation qualification exhausted its repetition deadline after the clean baseline',
      failures: ['mutation qualification exhausted its repetition deadline after the clean baseline'],
      outcome: 'incomplete', mutations: { caught: 0, total: 0 } };
  }
  preflightParallelMutationResources(args);
  const workerRoot = join(args.runsRoot, `r${repetition + 1}-workers`);
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
        { artifactPath, baselineBundle, repetition, workerIndex,
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
  const inspected = workers;
  const failures = inspected.flatMap(worker => worker.failures
    .map(failure => `worker ${worker.workerIndex + 1}: ${failure}`));
  if (cancellation.signal.aborted) failures.unshift(cancellationReason);
  let results = [];
  try {
    results = parallelMutationResults(manifest, inspected);
  } catch (error) { failures.push(error.message); }
  const representative = clean;
  const caught = results.filter(result => result.status === 'CAUGHT').length;
  if (results.length && caught !== results.length) failures.push('one or more mutations were not cleanly caught');
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
    output: relative(args.artifactDirectory, workerRoot).replaceAll('\\', '/'),
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
      artifact: relative(args.artifactDirectory, worker.artifactPath).replaceAll('\\', '/'),
      mutationIds: worker.assigned, ok: worker.failures.length === 0,
      logs: Object.fromEntries(Object.entries(worker.processResult.logs ?? {})
        .map(([name, log]) => [name, relative(args.artifactDirectory, log.path).replaceAll('\\', '/')])) })),
  };
}

export function referenceRunFromMutationBaseline(artifactDirectory, mutationRun, fixture,
  { release, level, selectedCheckKeys }) {
  const output = mutationRun.baselineOutput ?? mutationRun.output;
  const audit = auditReferenceRun(resolve(artifactDirectory, output), fixture,
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

function finalizeQualificationArtifact(artifact, { referenceMutationOnly = false } = {}) {
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
  artifact.ok = complete
    && artifact.runs.every(run => run.ok) && artifact.stable && artifact.sameImage
    && artifact.sameHarness;
  artifact.completedAt = new Date().toISOString();
  return artifact;
}

export function qualificationArtifactsOk(artifact, companion = null) {
  return artifact?.ok === true && (companion === null || companion?.ok === true);
}

async function main() {
  const args = parseReferenceQualificationArgs(process.argv);
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) throw new Error(`reference registry is invalid:\n${validation.issues.join('\n')}`);
  const selection = resolveReferenceSelection(registry, args);
  const fixture = selection.fixture;
  const inspection = inspectImportedReference(fixture);
  if (!inspection.ok) throw new Error(`${fixture.id} import is invalid:\n${inspection.failures.join('\n')}`);
  const context = referenceQualificationContext(fixture, selection.recipe,
    { level: args.level, featureCatalog: args.featureCatalog });
  assertReleaseCandidateRepetitions(args, context.calibration);
  const selectedManifest = args.mutations
    ? qualificationMutationManifest(fixture, context, args.mutationIds) : null;
  const runContext = args.mutationIds.length
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
    mkdirSync(args.mutationCheckpointDir, { recursive: true });
    if (args.mutationWorkers === 1 && !args.mutationCheckpoint) {
      args.mutationCheckpoint = join(args.mutationCheckpointDir, `${args.backend}-worker-1.json`);
    }
  }
  const selectedReference = context.calibration.references.entries.find(entry =>
    entry.backend === fixture.backend && entry.id === fixture.id);
  const selectedMutation = args.mutations
    ? { ...context.calibration.mutations.find(entry => entry.backend === fixture.backend),
      executionSha256: mutationExecutionSha256(selectedManifest) }
    : null;
  const qualificationRelease = referenceQualificationRelease(runContext.binding.release,
    runContext.selectedCheckKeys);
  const qualificationScope = qualificationScopeIdentity({
    kind: args.mutations ? 'mutation' : 'reference',
    release: qualificationRelease,
    stack: fixture.backend,
    reference: selectedReference,
    mutation: selectedMutation,
    stackBenchRoot: ROOT,
  });
  const artifact = { id, kind: 'reference_qualification', fixture: fixture.id,
    identities: emptyArtifactIdentities({
      fixture: { id: fixture.id, sha256: fixture.imported.sourceSha256, state: fixture.status },
      recipe: { id: context.binding.release.id, version: context.binding.release.version,
        sha256: context.binding.release.contentSha256, state: context.binding.release.state },
      calibration: { ...context.identity, state: context.calibration.state },
      stackAdapter: { id: fixture.backend },
    }),
    fixtureSha256: fixture.imported.sourceSha256, requiredRepetitions: args.repetitions,
    startedAt: new Date().toISOString(), isolation: 'docker',
    runner: controllerRunner(), qualificationScope, mutationControl: args.mutations,
    diagnostic: args.mutationIds.length > 0,
    qualifiedCheckKeys: [...runContext.selectedCheckKeys].sort(),
    ...(runContext.featureCatalog ? { featureCatalog: runContext.featureCatalog } : {}), runs: [] };
  const companion = companionPath ? {
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
  const artifactIdentities = artifact.identities;
  for (let repetition = 0; repetition < args.repetitions; repetition++) {
    console.log(`\nqualifying ${fixture.id}: clean run ${repetition + 1}/${args.repetitions}`);
    let companionCaptured = false;
    const captureCompanion = cleanRun => {
      if (!companion || companionCaptured) return;
      companion.runs.push(referenceRunFromMutationBaseline(args.artifactDirectory, cleanRun,
        fixture, { release: context.binding.release, level: args.level,
          selectedCheckKeys: runContext.selectedCheckKeys }));
      finalizeQualificationArtifact(companion);
      writeRunJson(companionPath, companion);
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
    { referenceMutationOnly: args.referenceMutationOnly });
  writeRunJson(paths.artifactPath, artifact);
  if (companion) {
    finalizeQualificationArtifact(companion);
    writeRunJson(companionPath, companion);
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
