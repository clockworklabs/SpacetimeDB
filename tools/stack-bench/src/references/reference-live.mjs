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
  writeSync } from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { executeStackCapability } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeRunJson } from '../evidence/artifacts.mjs';
import { hashDirectory } from '../evidence/provenance.mjs';
import { inspectImportedReference, loadReferenceRegistry, prepareReferenceFixtureSource,
  validateReferenceRegistry } from './reference-fixtures.mjs';
import { resolveReferenceSelection } from './reference-selection.mjs';
import { killTree } from '../runtime/platform.mjs';
import { criterionEvidence, evidencePassed } from '../evidence/check-evidence.mjs';
import { recoverSupervisedRun, validateSupervisorState } from '../runtime/recovery.mjs';
import { calibrationQualificationIdentity, resolveCalibrationForRelease } from '../composition/calibration-compiler.mjs';
import { resolveRecipeRelease } from '../composition/recipe-release.mjs';
import { isModularRecipeRelease } from '../composition/recipe-selection.mjs';
import { isDeclaredLevel, listTracks, loadTrack } from '../composition/tracks.mjs';
import { RUN_INDEX_CAP } from '../composition/tracks.mjs';
import { controllerRunner } from '../runtime/runner-environment.mjs';
import { mergeMutationShards, mutationWorkerSlots } from '../evidence/mutation-shards.mjs';

export { controllerRunner as referenceQualificationRunner } from '../runtime/runner-environment.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../project-paths.mjs';
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
    mutationWorkers: 1, mutationShardIndex: null, mutationShardCount: null };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--backend') args.backend = argv[++i];
    else if (argv[i] === '--track') args.track = argv[++i];
    else if (argv[i] === '--level') args.level = Number(argv[++i]);
    else if (argv[i] === '--recipe') args.recipe = argv[++i];
    else if (argv[i] === '--repetitions') args.repetitions = Number(argv[++i]);
    else if (argv[i] === '--run-index') args.runIndex = Number(argv[++i]);
    else if (argv[i] === '--spacetime-port') {
      args.spacetimePort = Number(argv[++i]);
      args.spacetimePortExplicit = true;
    }
    else if (argv[i] === '--timeout-minutes') args.timeoutMinutes = Number(argv[++i]);
    else if (argv[i] === '--mutations') args.mutations = true;
    else if (argv[i] === '--mutation-workers') args.mutationWorkers = Number(argv[++i]);
    else if (argv[i] === '--mutation-shard-index') args.mutationShardIndex = Number(argv[++i]);
    else if (argv[i] === '--mutation-shard-count') args.mutationShardCount = Number(argv[++i]);
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
  args.timeoutMinutes ??= args.mutations ? 90 : 60;
  if (!Number.isFinite(args.timeoutMinutes) || args.timeoutMinutes < 10 || args.timeoutMinutes > 240) {
    throw new Error('--timeout-minutes must be from 10 through 240');
  }
  args.timeoutMs = Math.round(args.timeoutMinutes * 60_000);
  return args;
}

export function runBounded(command, argv,
  { cwd, env, stdio = 'inherit', timeoutMs, terminate = killTree, logs = null, signal = null }) {
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
    const stop = () => {
      terminate(child.pid);
      try { child.kill('SIGKILL'); } catch { /* already exited */ }
    };
    const cancel = () => { cancelled = true; stop(); };
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
  { requireMutationControl = false, release = null } = {}) {
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
  const level = run.levels?.find(candidate => candidate.level === fixture.level);
  if (!level) failures.push(`L${fixture.level} result is missing`);
  else {
    if (!level.graded) failures.push(`L${fixture.level} was not graded`);
    if (!level.contractPass) failures.push(`L${fixture.level} contract lint failed`);
    if (level.score !== level.max) failures.push(`L${fixture.level} score is ${level.score}/${level.max}`);
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
    const expectedChecks = order(release.checkCatalog);
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
  return { ok: failures.length === 0, failures, runId: run.id, score: level ? `${level.score}/${level.max}` : null,
    imageId: run.setup?.isolation?.imageId ?? null, criteria: criteria.length,
    zeroPointCriteria: criteria.filter(criterion => criterion.points === 0).length, fingerprint,
    outcome: run.outcome?.kind ?? null, packRuntime: bundle.packRuntime ?? null,
    mutations: mutationControl?.summary ?? null };
}

export function referenceQualificationContext(fixture, recipe = null) {
  const track = loadTrack(fixture.track);
  const binding = resolveRecipeRelease(track, fixture.level, recipe);
  if (!binding) throw new Error(`${fixture.track} L${fixture.level} has no recipe release`);
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, stackBenchRoot: ROOT });
  if (!calibration) throw new Error(`${binding.release.id}@${binding.release.version} has no calibration`);
  const reference = calibration.references.entries.find(entry => entry.backend === fixture.backend
    && entry.id === fixture.id && entry.sourceSha256 === fixture.imported.sourceSha256);
  if (!reference) throw new Error(`${fixture.id} is not selected by calibration ${calibration.id}`);
  return { binding, calibration, identity: calibrationQualificationIdentity(calibration) };
}

export function referenceQualificationSelectionArgs(binding) {
  if (!binding?.release?.checkCatalog?.length) {
    throw new Error('reference qualification requires an exact recipe check catalog');
  }
  const args = ['--check', binding.release.checkCatalog.map(check => check.stableKey).join(',')];
  if (!isModularRecipeRelease(binding.release)) return args;
  const features = binding.release.components.packs
    .filter(pack => pack.moduleType === 'feature').map(pack => pack.id);
  const specifications = binding.release.components.packs
    .filter(pack => pack.moduleType === 'specification').map(pack => `${pack.id}@${pack.version}`);
  if (!features.length || !specifications.length) {
    throw new Error('modular reference qualification requires feature and specification modules');
  }
  return ['--feature-module', features.join(','), '--expect-spec', specifications.join(','), ...args];
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

export function referenceQualificationWorkRoot(env = process.env) {
  return resolve(env.STACK_BENCH_WORK_DIR ?? tmpdir());
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
  let processError = null;
  let cleanupPending = false;
  try {
    prepareReferenceFixtureSource(fixture, app);
    const adapter = STACK_ADAPTER_REGISTRY.get(fixture.backend);
    const env = { ...process.env, STACK_BENCH_SUPERVISOR_STATE: supervisorState,
      ...executeStackCapability(adapter, 'run-policy', 'supervisor-env',
        { spacetimePort: args.spacetimePort }) };
    const benchArgs = [BENCH, '--backend', fixture.backend, '--track', fixture.track,
      '--levels', String(fixture.level), '--run-index', String(args.runIndex), '--fix-rounds', '0',
      '--app', app, '--out', output, '--agent-adapter', 'reference-fixture', '--skip-probe', '--no-media'];
    benchArgs.push('--recipe', `${context.binding.release.id}@${context.binding.release.version}`);
    benchArgs.push(...referenceQualificationSelectionArgs(context.binding));
    benchArgs.push('--parent-attempt-id', id);
    if (args.mutations) {
      if (fixture.mutationManifests.length !== 1) {
        throw new Error(`${fixture.id} must own exactly one mutation manifest for live mutation qualification`);
      }
      benchArgs.push('--mutations', join(ROOT, fixture.mutationManifests[0]));
      if (args.mutationShardCount !== null) {
        benchArgs.push('--mutation-shard-index', String(args.mutationShardIndex),
          '--mutation-shard-count', String(args.mutationShardCount));
      }
    }
    const child = await runBounded(process.execPath, benchArgs,
      { cwd: ROOT, stdio: 'inherit', env, timeoutMs: args.timeoutMs });
    if (!child.ok) {
      const reason = child.timedOut
        ? `benchmark exceeded ${args.timeoutMinutes} minute repetition deadline`
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
  const audit = auditReferenceRun(output, fixture, {
    requireMutationControl: args.mutations,
    release: context.binding.release,
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
  { artifactPath, repetition, workerIndex, workerCount }) {
  const runIndex = args.runIndex + workerIndex;
  const argv = [fileURLToPath(import.meta.url), '--backend', args.backend,
    '--track', args.track, '--level', String(args.level), '--recipe',
    `${context.binding.release.id}@${context.binding.release.version}`,
    '--repetitions', '1', '--run-index', String(runIndex), '--timeout-minutes',
    String(args.timeoutMinutes), '--mutations', '--mutation-shard-index',
    String(workerIndex), '--mutation-shard-count', String(workerCount), '--out', artifactPath];
  if (args.spacetimePortExplicit) {
    argv.push('--spacetime-port', String(args.spacetimePort + workerIndex));
  }
  return argv;
}

function identityKey(identity) {
  return JSON.stringify({ id: identity?.id ?? null, version: identity?.version ?? null,
    sha256: identity?.sha256 ?? null, state: identity?.state ?? null });
}

export function readParallelMutationWorker(path, processResult, expected, manifest) {
  const failures = [];
  if (!processResult.ok) {
    failures.push(processResult.timedOut ? 'worker timed out'
      : processResult.error?.message ?? `worker exited ${processResult.code ?? processResult.signal}`);
  }
  if (!existsSync(path)) return { failures: [...failures, 'worker artifact is missing'] };
  let artifact;
  try { artifact = readArtifact(path); }
  catch (error) { return { failures: [...failures, `worker artifact is invalid: ${error.message}`] }; }
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
  const assigned = manifest.mutations
    .filter((_, position) => position % expected.workerCount === expected.workerIndex)
    .map(mutation => mutation.id);
  if (control && (control.shard?.index !== expected.workerIndex
      || control.shard?.count !== expected.workerCount
      || JSON.stringify(control.shard?.mutationIds) !== JSON.stringify(assigned))) {
    failures.push('worker mutation shard does not match its assignment');
  }
  if (control?.ok !== true) failures.push('worker mutation control did not pass');
  return { artifact, payload, run, control, failures };
}

async function runParallelMutationRepetition(fixture, args, context, id, repetition, artifactIdentities) {
  if (fixture.mutationManifests.length !== 1) {
    throw new Error(`${fixture.id} must own exactly one mutation manifest for parallel qualification`);
  }
  const manifest = readJson(join(ROOT, fixture.mutationManifests[0]));
  if (!Array.isArray(manifest.mutations) || manifest.mutations.length < args.mutationWorkers) {
    throw new Error(`--mutation-workers cannot exceed ${manifest.mutations?.length ?? 0} mutations`);
  }
  const started = Date.now();
  const workerRoot = join(args.runsRoot, `r${repetition + 1}-workers`);
  mkdirSync(workerRoot, { recursive: true });
  const cancellation = new AbortController();
  const cancel = () => cancellation.abort();
  process.once('SIGINT', cancel);
  process.once('SIGTERM', cancel);
  let workers;
  try {
    workers = await Promise.all(Array.from({ length: args.mutationWorkers }, async (_, workerIndex) => {
      const artifactPath = join(workerRoot, `w${workerIndex + 1}.json`);
      const logs = { stdout: join(workerRoot, `w${workerIndex + 1}.stdout.log`),
        stderr: join(workerRoot, `w${workerIndex + 1}.stderr.log`) };
      const argv = parallelMutationChildArgv(args, context,
        { artifactPath, repetition, workerIndex, workerCount: args.mutationWorkers });
      const processResult = await runBounded(process.execPath, argv,
        { cwd: ROOT, env: process.env, timeoutMs: args.timeoutMs + 5 * 60_000, logs,
          signal: cancellation.signal });
      return { workerIndex, runIndex: args.runIndex + workerIndex, artifactPath, logs, processResult };
    }));
  } finally {
    process.removeListener('SIGINT', cancel);
    process.removeListener('SIGTERM', cancel);
  }
  const inspected = workers.map(worker => ({ ...worker, ...readParallelMutationWorker(
    worker.artifactPath, worker.processResult,
    { ...artifactIdentities, workerIndex: worker.workerIndex,
      workerCount: args.mutationWorkers }, manifest) }));
  const failures = inspected.flatMap(worker => worker.failures
    .map(failure => `worker ${worker.workerIndex + 1}: ${failure}`));
  if (cancellation.signal.aborted) failures.unshift('parallel mutation qualification was interrupted');
  let results = [];
  try {
    results = mergeMutationShards(manifest.mutations, inspected.map(worker => ({
      index: worker.control?.shard?.index,
      count: worker.control?.shard?.count,
      mutationIds: worker.control?.shard?.mutationIds,
      results: worker.control?.results,
    })));
  } catch (error) { failures.push(error.message); }
  const representative = inspected.find(worker => worker.run)?.run ?? {};
  const caught = results.filter(result => result.status === 'CAUGHT').length;
  if (results.length && caught !== results.length) failures.push('one or more mutations were not cleanly caught');
  const fingerprints = new Set(inspected.map(worker => worker.run?.fingerprint).filter(Boolean));
  const images = new Set(inspected.map(worker => worker.run?.imageId).filter(Boolean));
  const harnesses = new Set(inspected.flatMap(worker =>
    [worker.run?.harnessSha256Before, worker.run?.harnessSha256After]).filter(Boolean));
  if (fingerprints.size !== 1) failures.push('worker baseline fingerprints differ');
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
    workers: inspected.map(worker => ({ index: worker.workerIndex, runIndex: worker.runIndex,
      artifact: relative(args.artifactDirectory, worker.artifactPath).replaceAll('\\', '/'),
      mutationIds: worker.control?.shard?.mutationIds ?? [], ok: worker.failures.length === 0,
      logs: Object.fromEntries(Object.entries(worker.processResult.logs ?? {})
        .map(([name, log]) => [name, relative(args.artifactDirectory, log.path).replaceAll('\\', '/')])) })),
  };
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
  const context = referenceQualificationContext(fixture, selection.recipe);

  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const id = `reference-live-${fixture.backend}-${stamp}-${process.pid}`;
  const paths = referenceQualificationPaths(args, id);
  if (existsSync(paths.artifactPath)) {
    throw new Error(`refusing to replace existing qualification artifact: ${paths.artifactPath}`);
  }
  if (existsSync(paths.runsRoot)) {
    throw new Error(`refusing to reuse existing qualification run directory: ${paths.runsRoot}`);
  }
  args.artifactDirectory = paths.artifactDirectory;
  args.runsRoot = paths.runsRoot;
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
    runner: controllerRunner(), mutationControl: args.mutations, runs: [] };
  const artifactIdentities = artifact.identities;
  for (let repetition = 0; repetition < args.repetitions; repetition++) {
    console.log(`\nqualifying ${fixture.id}: clean run ${repetition + 1}/${args.repetitions}`);
    const run = args.mutationWorkers > 1
      ? await runParallelMutationRepetition(fixture, args, context, id, repetition,
        artifactIdentities)
      : await runOnce(fixture, args, context, id, repetition);
    artifact.runs.push(run);
    // Repetition measures stability of a passing baseline. Repeating a setup or
    // infrastructure failure only wastes time and produces duplicate noise.
    if (!run.ok) break;
  }
  const complete = artifact.runs.length === args.repetitions;
  const fingerprints = new Set(artifact.runs.map(run => run.fingerprint).filter(Boolean));
  const images = new Set(artifact.runs.map(run => run.imageId).filter(Boolean));
  const harnessHashes = new Set(artifact.runs.flatMap(run =>
    [run.harnessSha256Before, run.harnessSha256After]).filter(Boolean));
  artifact.stable = complete && fingerprints.size === 1 && artifact.runs.every(run => run.fingerprint);
  artifact.sameImage = complete && images.size === 1 && artifact.runs.every(run => run.imageId);
  artifact.sameHarness = complete && harnessHashes.size === 1;
  artifact.harnessSha256 = artifact.sameHarness ? [...harnessHashes][0] : null;
  artifact.ok = complete
    && artifact.runs.every(run => run.ok) && artifact.stable && artifact.sameImage && artifact.sameHarness;
  artifact.completedAt = new Date().toISOString();
  writeRunJson(paths.artifactPath, artifact);
  console.log(JSON.stringify({ ok: artifact.ok, artifact: paths.artifactPath, stable: artifact.stable,
    sameImage: artifact.sameImage, sameHarness: artifact.sameHarness,
    runs: artifact.runs.map(({ repetition, ok, score, criteria,
      zeroPointCriteria, mutations, failures }) => ({ repetition, ok, score, criteria, zeroPointCriteria,
      mutations, failures })) }, null, 2));
  if (!artifact.ok) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 2; });
}
