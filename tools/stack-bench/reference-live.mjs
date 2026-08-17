#!/usr/bin/env node
// Repeatable live qualification for an imported reference fixture.
//
// A reference is promotable only after independent, clean Docker runs agree
// and every criterion passes, including zero-point controls. bench.mjs remains
// the lifecycle owner; this wrapper supplies fresh copies and audits the
// resulting evidence instead of trusting the benchmark process exit code.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, cpSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync,
  writeSync } from 'node:fs';
import { basename, dirname, extname, join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeRunJson } from './artifacts.mjs';
import { hashDirectory } from './provenance.mjs';
import { inspectImportedReference, loadReferenceRegistry, validateReferenceRegistry } from './reference-fixtures.mjs';
import { killTree } from './platform.mjs';
import { criterionEvidence, evidencePassed } from './check-evidence.mjs';
import { recoverSupervisedRun, validateSupervisorState } from './recovery.mjs';
import { calibrationQualificationIdentity, resolveCalibrationForRelease } from './calibration-compiler.mjs';
import { resolveRecipeRelease } from './recipe-release.mjs';
import { isDeclaredLevel, listTracks, loadTrack } from './tracks.mjs';
import { controllerRunner } from './runner-environment.mjs';

export { controllerRunner as referenceQualificationRunner } from './runner-environment.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const BENCH = join(ROOT, 'bench.mjs');

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
    runIndex: 0, spacetimePort: 3310, timeoutMinutes: 60, mutations: false };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--backend') args.backend = argv[++i];
    else if (argv[i] === '--track') args.track = argv[++i];
    else if (argv[i] === '--level') args.level = Number(argv[++i]);
    else if (argv[i] === '--recipe') args.recipe = argv[++i];
    else if (argv[i] === '--repetitions') args.repetitions = Number(argv[++i]);
    else if (argv[i] === '--run-index') args.runIndex = Number(argv[++i]);
    else if (argv[i] === '--spacetime-port') args.spacetimePort = Number(argv[++i]);
    else if (argv[i] === '--timeout-minutes') args.timeoutMinutes = Number(argv[++i]);
    else if (argv[i] === '--mutations') args.mutations = true;
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
  if (!Number.isInteger(args.repetitions) || args.repetitions < 2) {
    throw new Error('--repetitions must be an integer of at least 2');
  }
  if (!Number.isInteger(args.runIndex) || args.runIndex < 0) {
    throw new Error('--run-index must be a non-negative integer');
  }
  if (!Number.isInteger(args.spacetimePort) || args.spacetimePort < 1024 || args.spacetimePort > 65535) {
    throw new Error('--spacetime-port must be an integer from 1024 through 65535');
  }
  if (!Number.isFinite(args.timeoutMinutes) || args.timeoutMinutes < 10 || args.timeoutMinutes > 240) {
    throw new Error('--timeout-minutes must be from 10 through 240');
  }
  args.timeoutMs = Math.round(args.timeoutMinutes * 60_000);
  return args;
}

export function runBounded(command, argv,
  { cwd, env, stdio = 'inherit', timeoutMs, terminate = killTree, logs = null }) {
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
    let spawnError = null;
    const timer = setTimeout(() => {
      timedOut = true;
      // SIGTERM handlers are bypassed by Node on Windows. Kill the exact child
      // tree, then let the lease-aware supervisor remove detached resources.
      terminate(child.pid);
      try { child.kill('SIGKILL'); } catch { /* already exited */ }
    }, timeoutMs);
    child.once('error', error => { spawnError = error; });
    child.once('close', (code, signal) => {
      clearTimeout(timer);
      const captured = streams ? Object.fromEntries(Object.entries(streams).map(([name, state]) => {
        closeSync(state.fd);
        return [name, { path: state.path, sha256: state.hash.digest('hex'), bytes: state.bytes,
          retainedBytes: state.retainedBytes, truncated: state.bytes > state.retainedBytes }];
      })) : null;
      resolveRun({ ok: !timedOut && !spawnError && code === 0, code, signal, timedOut,
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

export function auditReferenceRun(output, fixture, { requireMutationControl = false } = {}) {
  const runPath = join(output, 'run.json');
  const bundlePath = join(output, 'grading', 'bundle.json');
  if (!existsSync(runPath) || !existsSync(bundlePath)) {
    return { ok: false, failures: ['run.json or grading/bundle.json is missing'] };
  }
  const run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
  const bundle = readArtifactPayload(bundlePath, { expectedKind: 'grade_bundle' });
  const failures = [];
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
        criteria.push({ key, points: criterion.points ?? 0, passed: evidencePassed(evidence),
          status: evidence.status });
        if (!evidencePassed(evidence)) failures.push(`${key} did not pass`);
      }
    }
  }
  if (!criteria.length) failures.push('no scenario criteria were recorded');
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

async function runOnce(fixture, args, id, repetition) {
  const workRoot = referenceQualificationWorkRoot();
  mkdirSync(workRoot, { recursive: true });
  const work = mkdtempSync(join(workRoot, `reference-live-${fixture.backend}-`));
  const app = join(work, 'app');
  const output = join(args.runsRoot, `r${repetition + 1}`);
  const supervisorState = join(work, 'supervisor-state.json');
  const started = Date.now();
  const harnessBefore = qualificationInputs();
  let processError = null;
  try {
    cpSync(join(ROOT, fixture.targetPath), app, { recursive: true });
    const adapter = STACK_ADAPTER_REGISTRY.get(fixture.backend);
    const env = { ...process.env, STACK_BENCH_SUPERVISOR_STATE: supervisorState,
      ...executeStackCapability(adapter, 'run-policy', 'supervisor-env',
        { spacetimePort: args.spacetimePort }) };
    const benchArgs = [BENCH, '--backend', fixture.backend, '--track', fixture.track,
      '--levels', String(fixture.level), '--run-index', String(args.runIndex), '--fix-rounds', '0',
      '--app', app, '--out', output, '--agent-adapter', 'reference-fixture', '--skip-probe', '--no-media'];
    if (args.recipe) benchArgs.push('--recipe', args.recipe);
    benchArgs.push('--parent-attempt-id', id);
    if (args.mutations) {
      if (fixture.mutationManifests.length !== 1) {
        throw new Error(`${fixture.id} must own exactly one mutation manifest for live mutation qualification`);
      }
      benchArgs.push('--mutations', join(ROOT, fixture.mutationManifests[0]));
    }
    const child = await runBounded(process.execPath, benchArgs,
      { cwd: ROOT, stdio: 'inherit', env, timeoutMs: args.timeoutMs });
    if (!child.ok) {
      const reason = child.timedOut
        ? `benchmark exceeded ${args.timeoutMinutes} minute repetition deadline`
        : child.error?.message ?? `benchmark exited ${child.code ?? child.signal ?? 'without status'}`;
      try { rescueSupervisedLease(supervisorState, output); }
      catch (cleanupError) { throw new Error(`${reason}; lease cleanup failed: ${cleanupError.message}`); }
      throw new Error(reason);
    }
  } catch (error) {
    processError = `${error.message}`;
  } finally {
    // This wrapper created exactly this mkdtemp directory. bench.mjs owns and
    // removes its leased container before returning; deleting the caller-owned
    // source copy here cannot target another run.
    try {
      rmSync(work, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    } catch (error) {
      // Cleanup evidence must not replace the benchmark failure that caused it.
      // Retain the exact owned path for diagnosis; a later run has a distinct
      // mkdtemp root and cannot accidentally adopt it.
      const cleanup = `owned temp cleanup failed (${error.code ?? 'unknown'}): ${work}`;
      processError = processError ? `${processError}; ${cleanup}` : cleanup;
    }
  }
  const audit = auditReferenceRun(output, fixture, { requireMutationControl: args.mutations });
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

async function main() {
  const args = parseReferenceQualificationArgs(process.argv);
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) throw new Error(`reference registry is invalid:\n${validation.issues.join('\n')}`);
  const fixture = registry.fixtures.find(candidate => candidate.backend === args.backend
    && candidate.track === args.track && candidate.level === args.level && candidate.status !== 'blocked');
  if (!fixture) throw new Error(`no imported ${args.track} L${args.level} fixture for ${args.backend}`);
  const inspection = inspectImportedReference(fixture);
  if (!inspection.ok) throw new Error(`${fixture.id} import is invalid:\n${inspection.failures.join('\n')}`);
  const context = referenceQualificationContext(fixture, args.recipe);

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
  for (let repetition = 0; repetition < args.repetitions; repetition++) {
    console.log(`\nqualifying ${fixture.id}: clean run ${repetition + 1}/${args.repetitions}`);
    const run = await runOnce(fixture, args, id, repetition);
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
