#!/usr/bin/env node
// Stack Bench: run the whole benchmark for one backend, unattended.
//
// For each level: build (or upgrade), grade, and if anything failed hand the
// agent a behavioural bug report and let it fix — up to --fix-rounds times —
// re-grading after each attempt. Records score, cost, time and fix rounds per
// level, then writes a summary.
//
// Usage:
//   node bench.mjs --backend spacetime --levels 1-5 [--model claude-sonnet-5]
//                  [--fix-rounds 3] [--run-index 0] [--out <dir>]
//                  [--retain-backend] [--no-media]
//
// The benchmark runs its own SpacetimeDB host (STACK_BENCH_STDB_URI, default
// 127.0.0.1:3210, data in .spacetime-data) rather than a machine-wide one, so
// resource measurements describe the module under test and a durability restart
// cannot disturb anything else. It is started if absent and stopped at the end
// unless --retain-backend.

import { execFile, execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, cpSync, rmSync, renameSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { loadTrack, resultsName, portsFor, workDirFor, assertNoPortCollisions,
  moduleName, dbName, DEFAULT_TRACK } from './tracks.mjs';
import { killTree, sleepSync } from './platform.mjs';
import { compareCriterionEvidence } from './scoring.mjs';
import { emptyArtifactIdentities, readArtifactPayload, writeArtifact, writeRunJson } from './artifacts.mjs';
import { aggregateRunOutcome, classifyBundle, mutationControlEligible, runExitCode } from './outcomes.mjs';
import { summarizeSessions } from './session-metrics.mjs';
import { hashDirectory } from './provenance.mjs';
import { createBackendLease, newRunId, publicBackendLease, readBackendLease,
  acquireResourceLock, releaseResourceLocks, updateBackendLease, writeBackendLease } from './backend-lease.mjs';
import { captureBackendDiagnostics } from './backend-control.mjs';
import { releaseBackendLease } from './backend-teardown.mjs';
import { resolveLegacyRecipeRelease } from './recipe-release.mjs';
import { resolveRecipeSelection } from './recipe-selection.mjs';
import { criterionEvidence, evidencePassed } from './check-evidence.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { agentRequestArgv, agentSessionFailure, validateAgentResult } from './agent-adapter-contract.mjs';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from './agent-adapters.mjs';
import { runPreflight } from './preflight.mjs';
import { DEFAULT_BUILD_IMAGE } from './product-config.mjs';
import { SUPERVISOR_STATE_VERSION, writeRecoveryArtifact } from './recovery.mjs';
import { resolveAgentCredential } from './agent-credentials.mjs';
import { sandboxProbeMode } from './sandbox.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const COMMAND_TIMEOUT_MS = 20 * 60_000;

function parseArgs(argv) {
  const a = { model: null, agentAdapter: 'claude-code',
    fixRounds: 3, runIndex: 0, levels: '1', media: true,
    guidance: 'prescribed', track: DEFAULT_TRACK, packIds: [], checkKeys: [] };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': a.backend = argv[++i]; break;
      case '--track': a.track = argv[++i]; break;
      case '--levels': a.levels = argv[++i]; break;
      case '--pack': a.packIds.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--check': a.checkKeys.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--model': a.model = argv[++i]; break;
      case '--fix-rounds': a.fixRounds = parseInt(argv[++i], 10); break;
      case '--max-budget-usd': a.maxBudgetUsd = Number(argv[++i]); break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--out': a.out = argv[++i]; break;
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--agent-adapter': a.agentAdapter = argv[++i]; break;
      case '--no-media': a.media = false; break;
      case '--retain-backend': a.retainBackend = true; break;
      case '--stack': a.guidance = argv[++i] === 'free' ? 'minimal' : 'prescribed'; break;
      case '--guidance': a.guidance = argv[++i]; break;
      case '--skip-probe': a.skipProbe = true; break;
      // Which reference documents to inline (spacetime only). The variable
      // under test in the cost work; passed straight through to agent.mjs.
      case '--skills': a.skills = argv[++i]; break;
      case '--api-key': a.apiKey = argv[++i]; break;
      case '--api-key-file': a.apiKeyFile = resolve(argv[++i]); break;
      case '--mutations': a.mutations = resolve(argv[++i]); break;
      // Start from an existing built app (a preserved L1 source) and UPGRADE it,
      // instead of rebuilding the lower level. The correct L1 that scored 51/51
      // is the right foundation for L2 — rebuilding it costs money and adds
      // variance that confounds the L1->L2 comparison.
      case '--seed-from': a.seedFrom = argv[++i]; break;
      case '--parent-attempt-id': a.parentAttemptId = argv[++i]; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.backend) {
    console.error('Usage: node bench.mjs --backend <b> --levels 1-3 [--fix-rounds 3] [--run-index N]');
    process.exit(2);
  }
  const [from, to] = a.levels.split('-').map(Number);
  a.levelList = Array.from({ length: (to ?? from) - from + 1 }, (_, i) => from + i);
  if (a.maxBudgetUsd !== undefined && (!Number.isFinite(a.maxBudgetUsd) || a.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  return a;
}

// Source only — node_modules and build output are large and reproducible.
//
// `client` is copied whole rather than cherry-picked. Naming src, index.html
// and vite.config.ts individually left out client/package.json and the
// tsconfigs, which meant the snapshot could not be installed, built or run —
// so the only runnable copy of any run was the stale results/<run>/app from an
// older layout, and an investigation that needed to RUN the app got pushed onto
// the wrong build. Evidence that cannot be executed is not evidence.
const SOURCE_DIRS = ['backend', 'server', 'client',
  // The back-office script is evidence — it is how each stack's model
  // interpreted "write the database directly", and the first run to require
  // one lost it to cleanup because it was not on this list.
  'scripts', 'package.json'];

function snapshotSource(appDir, to) {
  rmSync(to, { recursive: true, force: true });
  for (const rel of SOURCE_DIRS) {
    const from = join(appDir, rel);
    if (!existsSync(from)) continue;
    cpSync(from, join(to, rel), {
      recursive: true,
      // Both separators: on Windows the path is `client\dist\out.js`, which a
      // forward-slash-only class does not match, so build output was being
      // snapshotted here all along. It went unnoticed while `client` was
      // cherry-picked and dist was never walked.
      filter: src => !/node_modules|[\\/]dist([\\/]|$)/.test(src),
    });
  }
}

// Rolling back deletes the app's source, and a dev server watching that
// directory holds it open: an Express app under `server/` made rmSync throw
// EBUSY, which killed a finished postgres run outright — after grading, before
// its totals, transcripts or cleanup. The caller stops the servers first; this
// retries anyway, because a watcher can take a moment to let go and losing a
// completed run to a directory handle is a bad trade.
function restoreSource(from, appDir) {
  for (const rel of SOURCE_DIRS) {
    const src = join(from, rel);
    if (!existsSync(src)) continue;
    const dest = join(appDir, rel);

    // Keep the installed dependencies. A snapshot holds source only, so
    // deleting the whole directory and copying it back took node_modules with
    // it — the rolled-back app could no longer run, its database reset failed
    // with "you may have forgotten to install dependencies", and the level
    // ended NOT GRADED. A regressed fix round destroyed its own run instead of
    // falling back to the better source, which is the one thing rollback exists
    // to do.
    const mods = join(dest, 'node_modules');
    const parked = join(appDir, `.node_modules-${rel.replace(/[\\/]/g, '_')}`);
    let stashed = false;
    if (existsSync(mods)) {
      try { rmSync(parked, { recursive: true, force: true }); renameSync(mods, parked); stashed = true; }
      catch { /* fall through: a reinstall is better than a failed restore */ }
    }

    for (let attempt = 0; ; attempt++) {
      try { rmSync(dest, { recursive: true, force: true }); break; }
      catch (err) {
        if (attempt >= 5) throw err;
        sleepSync(2000);
      }
    }
    cpSync(src, dest, { recursive: true });

    if (stashed) {
      try { renameSync(parked, mods); }
      catch { rmSync(parked, { recursive: true, force: true }); }
    }
  }
}

// Did this build read the thing that grades it? Prevention has holes we know
// about — permission rules do not govern a bash `cat` — so this is checked
// after EVERY session rather than once at the end. A fix round that read the
// scenario file and ran grade.mjs was caught only in the closing summary, by
// which point the rollback grade had already been paid for and the level was
// unusable anyway. Catching it at the session that did it stops the spend.
function auditContamination(appDir) {
  try {
    const audit = sh('node', [join(ROOT, 'leak-audit.mjs'), '--app', appDir, '--json'], { stdio: 'pipe' });
    const escapes = JSON.parse(audit).flatMap(r => r.hits ?? []);
    const serious = escapes.filter(h => /GRADER|CONTRACT|BENCHMARK NOTES|PROMPTS/.test(h.kind));
    if (!serious.length) return null;
    return { evidence: [...new Set(serious.map(h => `${h.kind}: ${h.path.split('/').slice(-2).join('/')}`))].slice(0, 8),
      verdict: 'SCORES NOT USABLE — the build read the harness that grades it.' };
  } catch (e) {
    // An audit that could not run is not a pass.
    return { evidence: [`audit did not run: ${String(e.message).split(/\r?\n/)[0]}`],
      verdict: 'SCORES NOT USABLE — nothing verified this build stayed inside its directory.' };
  }
}

function containerIdentity(name) {
  try {
    const id = execFileSync('docker', ['inspect', '--format', '{{.Id}}', name],
      { encoding: 'utf8', stdio: 'pipe', timeout: 120_000 }).trim();
    if (!id) throw new Error('empty container id');
    return { name, id };
  } catch (error) {
    throw new Error(`cannot lease ${name}: ${String(error.message).split('\n')[0]}`);
  }
}

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, {
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: COMMAND_TIMEOUT_MS, ...opts,
  });

let activeAgentChild = null;
// Set once a run owns resources. The top-level rejection handler invokes this
// directly; relying only on process 'exit' made cleanup best-effort precisely
// when an awaited build rejected unexpectedly.
let emergencyTeardown = null;

function runAgent(args, adapter, mode, level, appDir) {
  const remainingBudget = args.maxBudgetUsd == null ? null
    : Number((args.maxBudgetUsd - (args.spentBudgetUsd ?? 0)).toFixed(6));
  if (remainingBudget !== null && remainingBudget <= 0) {
    throw new Error(`attempt cost cap of $${args.maxBudgetUsd} was exhausted before ${mode} L${level}`);
  }
  if (remainingBudget !== null && adapter.costLimit === 'unsupported') {
    throw new Error(`agent adapter ${adapter.id} cannot enforce --max-budget-usd`);
  }
  const request = { mode, level, app: appDir, backend: args.backend, track: args.track,
    runIndex: args.runIndex, model: args.model, guidance: args.guidance, skills: args.skills,
    maxBudgetUsd: remainingBudget, adapterCostLimit: adapter.costLimit };
  const argv = agentRequestArgv(adapter, request);
  if (args.apiKey && !adapter.apiKeyEnvironmentVariable) {
    throw new Error(`agent adapter ${adapter.id} does not accept an API key`);
  }
  const env = { ...process.env,
    ...(args.apiKey ? { [adapter.apiKeyEnvironmentVariable]: args.apiKey } : {}) };
  return new Promise((resolveRun, rejectRun) => {
    const child = execFile('node', argv, {
      encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: adapter.deadlineMs,
      env,
    },
      (error, stdout, stderr) => {
        if (activeAgentChild === child) activeAgentChild = null;
        if (error) {
          error.stdout = stdout;
          error.stderr = stderr;
          rejectRun(error);
          return;
        }
        try {
          const result = validateAgentResult(JSON.parse(stdout.trim().split('\n').pop()), request);
          args.spentBudgetUsd = Number(((args.spentBudgetUsd ?? 0) + result.costUsd).toFixed(6));
          resolveRun(result);
        }
        catch (parseError) {
          // Empty/malformed agent output used to discard stderr, turning a
          // failed deploy into the content-free claim "invalid JSON". Preserve
          // bounded tails from both pipes; they are the only evidence left once
          // teardown removes the build container.
          const stdoutTail = stdout.trim().slice(-2000) || '<empty>';
          const stderrTail = stderr.trim().slice(-4000) || '<empty>';
          rejectRun(new Error(`agent returned invalid JSON: ${parseError.message}\n`
            + `agent stdout tail:\n${stdoutTail}\nagent stderr tail:\n${stderrTail}`));
        }
      });
    activeAgentChild = child;
  });
}

function grade(args, appDir, url, label, level, track, parentAttemptId) {
  const restartSpec = restartSpecFor(args, appDir, track);
  const expressPort = restartSpec.port ?? '';
  const argv = [join(ROOT, 'run-suite.mjs'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--level', String(level),
    '--track', args.track,
    '--reseed-probe', `http://localhost:${expressPort}${track.restartProbe}`,
    '--run-index', String(args.runIndex),
    '--parent-attempt-id', parentAttemptId,
    ...args.packIds.flatMap(pack => ['--pack', pack]),
    ...args.checkKeys.flatMap(check => ['--check', check]),
    ...(args.media ? [] : ['--no-media']),
    ...(!executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
      'run-policy', 'reset-enabled')
      ? ['--no-reset']
      : ['--restart-spec', JSON.stringify(restartSpec)])];
  const bundle = join(appDir, 'stack-bench', 'bundle.json');
  rmSync(bundle, { force: true });
  try { sh('node', argv, { stdio: 'inherit' }); } catch { /* a current bundle may still explain a scored failure */ }
  return existsSync(bundle) ? readArtifactPayload(bundle, { expectedKind: 'grade_bundle' }) : null;
}

function restartSpecFor(args, appDir, track) {
  const port = portsFor(track, args.backend, args.runIndex).express ?? null;
  return { backend: args.backend, app: appDir, port: port == null ? null : Number(port),
    probe: track.restartProbe };
}

function runMutationControl(args, appDir, url, track) {
  const output = join(args.out, 'mutation-control.json');
  rmSync(output, { force: true });
  const argv = [join(ROOT, 'grader', 'mutation-test.mjs'), '--app', appDir,
    '--url', url, '--mutations', args.mutations, '--backend', args.backend,
    '--track', args.track, '--run-index', String(args.runIndex), '--out', output,
    '--restart-spec', JSON.stringify(restartSpecFor(args, appDir, track)),
    '--parent-attempt-id', args.parentAttemptId];
  let processError = null;
  try { sh(process.execPath, argv, { stdio: 'inherit' }); }
  catch (error) { processError = String(error.message).split('\n')[0]; }
  if (!existsSync(output)) {
    return { ok: false, artifact: output, processError,
      outcome: { kind: 'harness_failure', phase: 'mutation-control',
        reason: processError ?? 'mutation runner produced no artifact' } };
  }
  const artifact = readArtifactPayload(output, { expectedKind: 'mutation_control' });
  return { ok: artifact.ok === true && !processError, artifact: output,
    processError, summary: artifact.summary ?? null, outcome: artifact.outcome ?? null,
    results: artifact.results ?? [] };
}

function validateMutationInput(args) {
  if (!args.mutations) return;
  if (!args.app) throw new Error('--mutations requires an explicit pristine --app');
  const manifest = JSON.parse(readFileSync(args.mutations, 'utf8'));
  if (!/^[a-f0-9]{64}$/.test(manifest.fixtureSha256 ?? '')) {
    throw new Error('mutation manifest has no valid fixtureSha256');
  }
  const fixture = hashDirectory(args.app);
  if (fixture.sha256 !== manifest.fixtureSha256) {
    throw new Error(`mutation manifest targets fixture ${manifest.fixtureSha256}, not ${fixture.sha256}`);
  }
}

async function main() {
  const args = parseArgs(process.argv);
  const stackAdapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const agentAdapter = AGENT_ADAPTER_REGISTRY.get(args.agentAdapter);
  resolveAgentCredential(args, agentAdapter);
  args.model ??= agentAdapter.defaultModel;
  if (args.retainBackend
    && !executeStackCapability(stackAdapter, 'run-policy', 'retain-host-supported')) {
    throw new Error(`stack adapter ${args.backend} does not support --retain-backend`);
  }
  const stackRuntime = executeStackCapability(stackAdapter, 'orchestrator', 'config', {
    root: ROOT, env: process.env, helpers: { exists: existsSync },
  });
  Object.assign(process.env, stackRuntime.environment);
  process.env.STACK_BENCH_NODE_BIN = process.platform === 'win32' ? 'node.exe' : process.execPath;
  const track = loadTrack(args.track);
  // Resolve the requested scope for every level before probing the sandbox,
  // acquiring a backend lease or paying for a build. A pack that exists at L2
  // but not L1 is not a late grading surprise; it is an invalid run request.
  for (const level of args.levelList) {
    const binding = resolveLegacyRecipeRelease(track, level);
    if (!binding && (args.packIds.length || args.checkKeys.length)) {
      throw new Error(`L${level} has no recipe release, so --pack/--check cannot be resolved`);
    }
    if (binding) resolveRecipeSelection(binding.release, args);
  }
  // Caller-owned mutation inputs are pure request data. Reject them before
  // checking credentials, Docker, ports, or any other ambient runner state so
  // an invalid experiment can never be masked by an unrelated preflight error.
  validateMutationInput(args);
  assertNoPortCollisions();
  // The deterministic adapter/stack is the model-free unit loop. Real runs
  // prove the exact requested scope, engine, image, credentials, storage and
  // ports before the sandbox probe or any paid coding session begins.
  const preflight = args.backend === 'stub' ? null : runPreflight({
    backends: [args.backend], track: args.track, levels: args.levels,
    levelList: args.levelList, runIndex: args.runIndex, agentAdapter: args.agentAdapter,
    agentSkills: args.skills?.split(',').filter(Boolean) ?? null,
    packIds: args.packIds, checkKeys: args.checkKeys, smoke: true,
    supervisorState: process.env.STACK_BENCH_SUPERVISOR_STATE ?? null,
    image: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: resolve(args.out ?? process.env.STACK_BENCH_RESULTS_DIR ?? join(ROOT, 'results')),
  }, { env: args.apiKey && agentAdapter.apiKeyEnvironmentVariable
    ? { ...process.env, [agentAdapter.apiKeyEnvironmentVariable]: '<provided-by-argument>' }
    : process.env });
  if (preflight && !preflight.ok) {
    const failures = preflight.checks.filter(check => check.status === 'fail');
    console.error('\nPREFLIGHT FAILED — no model session was started.');
    for (const failure of failures) {
      console.error(`  ${failure.id}: ${failure.summary}`);
      if (failure.remediation) console.error(`    fix: ${failure.remediation}`);
    }
    process.exit(2);
  }
  if (preflight) console.log(`  preflight  ... ${preflight.summary.passed} checks passed`
    + `${preflight.summary.warnings ? `, ${preflight.summary.warnings} warning(s)` : ''}`);
  const beyondValidatedLevels = args.levelList.filter(level => level > track.validatedThrough);
  if (beyondValidatedLevels.length) {
    console.log(`  NOTICE: ${track.name} is validated through L${track.validatedThrough}; `
      + `this run also requests L${beyondValidatedLevels.join(', L')}. The result will record those exact levels.`);
  }

  // In the single-host topology, prove the sandbox before spending a run on it.
  // The rules have already been
  // wrong twice in ways that read as fine: a deny list shipped under
  // --dangerously-skip-permissions enforced nothing at all, and the first probe
  // to check it reported a pass by matching the word "denied" inside the file it
  // had just read. Neither showed up as an error — both would have produced a
  // full set of confident, void scores. In the appliance, the stronger boundary
  // is structural: the model runs in a separate container where the controller,
  // grader, scenarios, prior results, and Docker socket do not exist. Running
  // this probe in the controller would test the wrong image and trust zone.
  // The stub backend is the offline test loop: no model, no cost, nothing to
  // protect. Spending a real CLI session probing it would make the one test
  // that is supposed to run for free stop being free.
  const probeMode = sandboxProbeMode({ appliance: process.env.STACK_BENCH_APPLIANCE === '1',
    explicitlySkipped: args.skipProbe, stackRequired: executeStackCapability(stackAdapter,
      'run-policy', 'sandbox-probe-required') });
  if (probeMode === 'container-isolation') {
    console.log('  sandbox    ... coding container is isolated from the controller and grading files');
  } else if (probeMode === 'direct-cli') {
    console.log('  sandbox    ... probing the deny rules');
    try {
      sh('node', [join(ROOT, 'probe-sandbox.mjs'), '--mode', 'acceptEdits', '--model', args.model],
        { stdio: 'inherit' });
    } catch {
      console.error('\nSANDBOX PROBE FAILED — refusing to start a run whose scores could not be trusted.');
      console.error('Run `node probe-sandbox.mjs --mode acceptEdits` to see which path got through.');
      process.exit(2);
    }
  }
  const url = args.url ?? `http://localhost:${portsFor(track, args.backend, args.runIndex).vite}`;
  const runDir = resultsName(track, args.backend, args.runIndex);
  const runId = newRunId({ track: args.track, backend: args.backend, runIndex: args.runIndex });
  const artifactLabel = `${runDir}-${runId}`;
  // Default results never reuse a directory. The stable backend/run name is a
  // grouping directory only; every artifact beneath it belongs to one run id.
  args.out ??= join(process.env.STACK_BENCH_RESULTS_DIR ?? join(ROOT, 'results'), runDir, runId);
  mkdirSync(args.out, { recursive: true });
  if (existsSync(join(args.out, 'run.json'))) {
    throw new Error(`refusing to reuse result directory containing run.json: ${args.out}`);
  }
  if (preflight) writeArtifact(join(args.out, 'preflight.json'), {
    kind: 'preflight', id: `${runId}-preflight`,
    attempt: { id: `${runId}-preflight`, parentId: runId },
    identities: emptyArtifactIdentities({
      agentAdapter: agentAdapterIdentity(agentAdapter),
      stackAdapter: { id: stackAdapter.id, version: stackAdapter.version },
    }),
    payload: preflight,
  });

  // Resolve and validate caller-owned source before acquiring a backend slot.
  // A failed pristine-hash check used to happen after lease creation but before
  // teardown handlers existed, leaving a dead-owner lock and private lease.
  const ownWorkDir = !args.app;
  const appDir = args.app ?? join(workDirFor(track, args.backend, args.runIndex, runId), 'app');

  // Every destructive or lifecycle operation is tied to this record. A boolean
  // "owned" flag could say yes without identifying what was owned, and after a
  // restart it could not prove the listener being killed was the one this run
  // started. The token prevents a stale sibling process from presenting a
  // different lease accidentally; the targets themselves come only from the
  // lease, never from generated application files.
  const runtimeRoot = resolve(process.env.STACK_BENCH_RUNTIME_DIR
    ?? join(tmpdir(), 'stack-bench-runtime'));
  const runtimeDir = join(runtimeRoot, runId);
  const leasePath = join(runtimeDir, 'backend-lease.json');
  const preparedLease = executeStackCapability(stackAdapter, 'lease', 'prepare', {
    track,
    runIndex: args.runIndex,
    runtimeDir,
    serverUri: stackRuntime.lease.serverUri,
    env: process.env,
    helpers: { containerIdentity, dbName, moduleName },
  });
  const initialLease = createBackendLease({
    runId,
    backend: args.backend,
    track: args.track,
    runIndex: args.runIndex,
    ...preparedLease.lease,
  });
  const lockRoot = join(tmpdir(), 'stack-bench-resource-locks');
  const lockKeys = [`slot:${args.track}:${args.backend}:run${args.runIndex}`,
    ...preparedLease.lockKeys];
  let privateSupervisorStatePath = null;
  try {
    for (const key of lockKeys.sort()) {
      initialLease.resources.locks.push(acquireResourceLock({ root: lockRoot, key, lease: initialLease }));
    }
    writeBackendLease(leasePath, initialLease);
    const supervisorState = process.env.STACK_BENCH_SUPERVISOR_STATE
      ?? (process.env.STACK_BENCH_SUPERVISOR_DIR
        ? join(resolve(process.env.STACK_BENCH_SUPERVISOR_DIR), `${runId}.json`) : null);
    if (supervisorState) {
      // Private handoff to an outer timeout supervisor. It contains the lease
      // token, so create it once with owner-only permissions and never place it
      // in the results tree.
      privateSupervisorStatePath = resolve(supervisorState);
      mkdirSync(dirname(privateSupervisorStatePath), { recursive: true, mode: 0o700 });
      writeFileSync(privateSupervisorStatePath, `${JSON.stringify({
        version: SUPERVISOR_STATE_VERSION, runId, backend: args.backend, runtimeDir, leasePath,
        ownershipToken: initialLease.ownershipToken, output: resolve(args.out),
      })}\n`, { flag: 'wx', mode: 0o600 });
    }
  } catch (error) {
    releaseResourceLocks(initialLease);
    rmSync(runtimeDir, { recursive: true, force: true });
    throw error;
  }
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = initialLease.ownershipToken;
  if (process.platform === 'win32') {
    // `bash` resolves to WSL on this host. WSL drops ordinary Windows
    // environment additions unless WSLENV names them, and path-valued entries
    // need /p translation. Without this bridge the lifecycle script sees no
    // lease and correctly refuses every reset/restart.
    const bridge = ['STACK_BENCH_LEASE/p', 'STACK_BENCH_LEASE_TOKEN',
      'STACK_BENCH_NODE_BIN', ...stackRuntime.windowsEnvironmentBridge];
    const existing = (process.env.WSLENV ?? '').split(':').filter(Boolean);
    process.env.WSLENV = [...new Set([...existing, ...bridge])].join(':');
  }

  // The app used to live at results/<run>/app and now builds outside the
  // results tree, so anything still sitting there belongs to an older run under
  // the old layout. It is not harmless clutter: it looks exactly like this
  // run's application, sits directly beside `source/` which IS this run's
  // application, and answers questions about the wrong build without saying so.
  // One investigation compared a two-day-old app against a correctly-published
  // current module, concluded the schemas had drifted, and filed a defect
  // against SpacetimeDB that did not exist. Delete it on the way in.
  // Deleting is only safe where source/ already holds that run's code. Some
  // older runs have an app/ and no source/, and there app/ is the only copy
  // there is — those get renamed out of the way instead of destroyed.
  const staleApp = join(args.out, 'app');
  if (existsSync(staleApp)) {
    const supersededBySource = existsSync(join(args.out, 'source'));
    try {
      if (supersededBySource) {
        rmSync(staleApp, { recursive: true, force: true });
        console.log('  results    ... removed a stale app/ from an earlier run (source/ has that code)');
      } else {
        const parked = join(args.out, 'app-from-earlier-run');
        rmSync(parked, { recursive: true, force: true });
        renameSync(staleApp, parked);
        console.log('  results    ... an earlier run left app/ with no source/; kept it as app-from-earlier-run/');
      }
    } catch (err) {
      // Naming it is the point — a leftover nobody knows about is the hazard.
      console.log(`  results    ... WARNING: could not clear stale ${staleApp}: ${String(err.message).split('\n')[0]}`);
      console.log("               it is NOT this run's app — read source/ instead.");
    }
  }

  let tornDown = false;
  let activeRun = null;
  const recoveryPath = join(args.out, 'recovery.json');
  const writeLeaseEvidence = (knownLease = null) => {
    const lease = knownLease ?? readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId });
    const out = join(args.out, 'backend-lease.json');
    const evidence = publicBackendLease(lease);
    const id = `${runId}-backend-lease`;
    writeArtifact(out, {
      kind: 'backend_lease_evidence', id,
      attempt: { id, parentId: runId },
      timestamps: { startedAt: evidence.createdAt, completedAt: new Date().toISOString() },
      identities: emptyArtifactIdentities({ stackAdapter: { id: args.backend } }),
      payload: evidence,
    });
    return evidence;
  };
  const teardown = ({ reason = null, retainBackend = args.retainBackend } = {}) => {
    if (tornDown) return;
    if (activeAgentChild?.pid) {
      killTree(activeAgentChild.pid);
      activeAgentChild = null;
    }
    // Preserve restart failures before removing the only filesystem that holds
    // their stderr. A 500 after restart is otherwise impossible to distinguish
    // from an application defect, a dead dependency, or host pressure.
    if (activeRun) {
      try {
        activeRun.backendDiagnostics = captureBackendDiagnostics(join(args.out, 'backend.log'));
      } catch (error) {
        activeRun.backendDiagnostics = { captured: false,
          reason: String(error.message).split(/\r?\n/)[0] };
      }
    }
    let released = false;
    let cleanupError = null;
    try {
      released = releaseBackendLease(leasePath, initialLease.ownershipToken,
        { retainBackend });
    } catch (error) { cleanupError = error; }
    let finalLease = initialLease;
    try {
      finalLease = readBackendLease(leasePath,
        { token: initialLease.ownershipToken, backend: args.backend, runId });
    } catch (error) { cleanupError ??= error; released = false; }
    const evidence = writeLeaseEvidence(finalLease);
    writeRecoveryArtifact(recoveryPath, finalLease, { cleanupSucceeded: released,
      retained: Boolean(retainBackend),
      reason: cleanupError?.message ?? reason ?? (released ? null : 'authenticated cleanup refused') });
    if (activeRun) {
      activeRun.backendLease = evidence;
      activeRun.outcome ??= aggregateRunOutcome(activeRun.levels);
      writeRunJson(join(args.out, 'run.json'), activeRun);
    }
    tornDown = released;
    if (released && !retainBackend) {
      rmSync(runtimeDir, { recursive: true, force: true });
      if (privateSupervisorStatePath) rmSync(privateSupervisorStatePath, { force: true });
    }
    if (cleanupError) throw cleanupError;
    if (!released) throw new Error(`backend teardown refused: listener no longer matches lease ${runId}`);
  };
  emergencyTeardown = teardown;

  try {
    executeStackCapability(stackAdapter, 'lifecycle', 'activate', {
      leasePath, leaseToken: initialLease.ownershipToken, lease: initialLease,
      ...stackRuntime.lifecycle,
    });
  } catch (error) {
    try { teardown({ reason: `backend activation failed: ${error.message}`, retainBackend: false }); }
    catch (cleanupError) {
      console.error(`  activation cleanup quarantined: ${String(cleanupError.message).split(/\r?\n/)[0]}`);
    }
    throw error;
  }

  // One app, grown level by level — the same app the earlier levels built.
  // Built OUTSIDE the harness. While the app lived at results/<run>/app it sat
  // underneath the thing grading it: two directories up are the scenario files
  // and grade.mjs, and transcripts show builds taking exactly that walk. An
  // isolated root removes the class rather than forbidding instances of it.
  // Artifacts are copied back to results/ when the run finishes.
  // This run removes its own directory on normal teardown. It deliberately does
  // not sweep other run directories on startup: recursive deletion of an old
  // node_modules tree can monopolize Windows I/O, and age alone is not ownership
  // evidence when another agent is working in parallel.
  // Stamped, so this run cannot inherit a directory another one is still
  // holding. Every level shares it: L2 upgrades the app L1 built.
  //
  // An EXPLICIT --app belongs to the caller — the test loop passes one, and
  // deleting its parent on the way out deleted the loop's own run.json. Only a
  // directory this run created is this run's to remove.
  // Leave nothing running once the run is over, however it ends — but only stop
  // what this run brought up.
  // This run's work path is unique. There is no legitimate pre-existing build
  // container to delete; teardown removes one only after run-build records its
  // immutable id in the lease.
  const interrupt = (signal, exitCode) => {
    console.log(`interrupted by ${signal} — stopping exact owned resources`);
    try { teardown({ reason: `interrupted by ${signal}` }); }
    catch (error) { console.error(`  cleanup quarantined: ${String(error.message).split(/\r?\n/)[0]}`); }
    process.exit(exitCode);
  };
  process.on('SIGINT', () => interrupt('SIGINT', 130));
  process.on('SIGTERM', () => interrupt('SIGTERM', 143));
  process.on('exit', () => {
    if (!tornDown) {
      try { teardown(); } catch (error) {
        console.error(`  cleanup failed: ${String(error.message).split('\n')[0]}`);
      }
    }
  });

  // Seed the work dir from an existing app, so the first level upgrades it
  // rather than building from nothing. Source only (SOURCE_DIRS); the upgrade
  // session installs its own dependencies exactly as a developer checking out
  // the L1 code would.
  if (args.seedFrom) {
    const from = resolve(args.seedFrom);
    if (!existsSync(from)) { console.error(`--seed-from path does not exist: ${from}`); process.exit(2); }
    mkdirSync(appDir, { recursive: true });
    for (const rel of SOURCE_DIRS) {
      const src = join(from, rel);
      if (existsSync(src)) cpSync(src, join(appDir, rel), { recursive: true });
    }
    console.log(`  seeded from ${from} — level ${args.levelList[0]} will UPGRADE it, not rebuild`);
  }

  const started = Date.now();
  const run = { id: runId, startedAt: new Date(started).toISOString(),
    parentAttemptId: args.parentAttemptId ?? null,
    identities: emptyArtifactIdentities({
      agentAdapter: agentAdapterIdentity(agentAdapter),
      stackAdapter: { id: stackAdapter.id, version: stackAdapter.version },
    }),
    track: args.track, backend: args.backend, model: args.model,
    guidance: args.guidance, stack: args.guidance === 'minimal' ? 'free' : 'prescribed',
    skills: args.skills?.split(',').filter(Boolean) ?? [],
    runtime: { buildImage: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE },
    selectionRequest: { packs: [...args.packIds], checks: [...args.checkKeys] },
    backendLease: publicBackendLease(readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId })),
    validation: { validatedThrough: track.validatedThrough, beyondValidatedLevels }, levels: [] };
  activeRun = run;

  // A contaminated session ends the run at the session that did it. Continuing
  // spends money on a level whose score may not be quoted, and the previous
  // occurrence paid for a fix round, a grade, a rollback and a second grade
  // before the closing summary mentioned it. Exit 4 so a sweep records it as a
  // failure rather than a quiet zero.
  const abortContaminated = (whichSession, contamination) => {
    run.contaminated = true;
    run.contamination = { ...contamination, detectedAt: whichSession };
    console.log(`\n  !! CONTAMINATED at ${whichSession} — this session read the harness that grades it:`);
    for (const e of contamination.evidence) console.log(`     ${e}`);
    console.log('     Scores from this run must not be quoted. Stopping now rather than');
    console.log('     paying to grade a level that cannot be used.');
    try { writeRunJson(join(args.out, 'run.json'), run); } catch { /* best effort */ }
    try { sh('node', [join(ROOT, 'archive-transcripts.mjs'), '--app', appDir, '--label', artifactLabel], { stdio: 'pipe' }); } catch { /* best effort */ }
    teardown();
    process.exit(4);
  };

  for (const level of args.levelList) {
    const t0 = Date.now();
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    const firstMode = args.seedFrom ? 'upgrade' : 'build';
    const build = await runAgent(args, agentAdapter,
      level === args.levelList[0] ? firstMode : 'upgrade', level, appDir);
    const buildLeak = auditContamination(appDir);
    if (buildLeak) abortContaminated(`level ${level} build`, buildLeak);
    // Carry the agent's own record of the setup up to the run. Comparing two
    // scores is only meaningful if the reasoning budget, permission mode and
    // CLI version behind them were the same, and that is not knowable after the
    // fact unless it was written down at the time.
    run.setup ??= build.setup;
    // No session, no app. Grading an empty directory yields a real-looking zero
    // that is a harness failure, not a result for this backend.
    const buildFailure = agentSessionFailure(build);
    if (buildFailure) {
      console.log(`  ABORTED: ${buildFailure.reason} — see ${join(appDir, `.session-*-l${level}.json`)}`);
      run.levels.push({ level, score: null, max: null, error: buildFailure.reason,
        outcome: buildFailure,
        buildSession: { sessionId: build.sessionId ?? null, costUsd: build.costUsd,
          durationMs: build.durationMs, usage: build.usage ?? null,
          transcript: build.transcript ?? null, provenance: build.provenance ?? null,
          providerMetadata: build.providerMetadata ?? null },
        sessionTotals: summarizeSessions([build]),
        costUsd: build.costUsd, durationMs: Date.now() - t0 });
      break;
    }
    let bundle = grade(args, appDir, url, `${args.backend}-l${level}`, level, track, runId);

    // What the model built BEFORE being handed the answers. Every backend can
    // reach the same total given enough fix rounds, so the post-fix score stops
    // discriminating — what it got right unaided is the comparison that survives.
    const firstBuild = {
      score: bundle?.totals?.score ?? null,
      max: bundle?.totals?.max ?? null,
      contractPass: bundle?.totals?.contractPass ?? null,
      outcome: classifyBundle(bundle),
      missed: Object.values(bundle?.suites ?? {}).flatMap(s =>
        (s?.features ?? []).flatMap(f =>
          (f.criteria ?? []).filter(c => !evidencePassed(criterionEvidence(c)))
            .map(c => `${f.name}/${c.id}`))),
    };

    // Keep the first attempt before a fix round overwrites it.
    //
    // Until now a finished run kept only the FINAL source and the LAST grading
    // pass, so `firstBuild` above survived as a score and a list of criterion
    // ids and nothing else. That is the wrong thing to throw away: cost-to-
    // correct is mostly decided at the first attempt — SpacetimeDB opened at
    // 42/50 where PostgreSQL opened at 48/50 and MongoDB at 51/51 — so the
    // artifact that explains the headline number was the one being deleted.
    //
    // It has already blocked two diagnoses: whether SpacetimeDB's lost identity
    // points came from the SDK's reconnect defect or from the app never saving
    // its token, and what the TS2344 errors looked like in context. Both answers
    // were in a `main.tsx` that no longer existed.
    //
    // Source only, and grading without media — the same rules the end-of-run
    // copies use, so this adds a few MB per level rather than a run's worth of
    // traces and video.
    try {
      snapshotSource(appDir, join(args.out, `first-build-l${level}`));
      const gradingFrom = join(appDir, 'stack-bench');
      if (existsSync(gradingFrom)) {
        cpSync(gradingFrom, join(args.out, `first-build-l${level}-grading`), {
          recursive: true,
          filter: src => !/[\\/]media([\\/]|$)/.test(src),
        });
      }
      console.log(`  kept the unaided attempt at ${join(args.out, `first-build-l${level}`)}`);
    } catch (e) {
      // Never worth losing a run over: the score is already recorded.
      console.log(`  !! could not keep the first build: ${String(e.message).split('\n')[0]}`);
    }

    let fixRounds = 0;
    let fixCost = 0;
    const fixSessions = [];
    let stalled = false;
    let regressed = false;

    // Hand back findings and let the agent fix, until clean or out of rounds.
    while (fixRounds < args.fixRounds) {
      let wroteReport = true;
      try {
        sh('node', [join(ROOT, 'report-bugs.mjs'), '--app', appDir,
          '--archive', join(appDir, 'stack-bench', 'records',
            `bug-report-l${level}-round${fixRounds + 1}.md`)], { stdio: 'pipe' });
      } catch (err) {
        if (err.status === 3) wroteReport = false;      // nothing failed
        else throw err;
      }
      if (!wroteReport) break;

      const before = bundle?.totals?.score ?? 0;
      const beforeMax = bundle?.totals?.max ?? 0;
      // Kept whole, not just its total: the regression check compares
      // per-criterion, because totals are scored out of a denominator that
      // moves between rounds.
      const beforeBundle = bundle;
      // A fix can break more than it mends. Keep the source that produced the
      // best score so far, and roll back to it if a round regresses.
      // Kept outside the results tree: a snapshot is a known-good copy of the
      // answer, and a coding session that can reach one will copy it instead of
      // building. It only has to survive this process.
      const snapshot = join(tmpdir(), `stack-bench-snapshot-${args.backend}-${args.track}-run${args.runIndex}-l${level}`);
      snapshotSource(appDir, snapshot);
      fixRounds += 1;
      console.log(`--- fix round ${fixRounds}/${args.fixRounds} ---`);
      const fix = await runAgent(args, agentAdapter, 'fix', level, appDir);
      fixCost += fix.costUsd;
      fixSessions.push({ round: fixRounds, sessionId: fix.sessionId ?? null,
        costUsd: fix.costUsd, durationMs: fix.durationMs, usage: fix.usage ?? null,
        tokens: fix.tokens ?? null, outputTokens: fix.outputTokens ?? null,
        turns: fix.turns ?? null, promptBytes: fix.promptBytes ?? null,
        thinking: fix.thinking ?? null, transcript: fix.transcript ?? null,
        provenance: fix.provenance ?? null, providerMetadata: fix.providerMetadata ?? null });

      const fixFailure = agentSessionFailure(fix);
      if (fixFailure) {
        console.log(`    coding session failed: ${fixFailure.reason}; stopping repairs`);
        bundle = { outcome: fixFailure };
        break;
      }

      // Check the round that just ran, before paying to grade it. A fix session
      // that read the scenario file is not going to be redeemed by another
      // round, and grading it only produces a number nobody may quote.
      const fixLeak = auditContamination(appDir);
      if (fixLeak) abortContaminated(`fix round ${fixRounds}`, fixLeak);
      bundle = grade(args, appDir, url, `${args.backend}-l${level}-fix${fixRounds}`, level, track, runId);

      // A round that moves nothing usually means the finding is not actionable —
      // often the harness is wrong, not the app. Stop rather than pay again for
      // the same result.
      const after = bundle?.totals?.score ?? 0;
      const afterMax = bundle?.totals?.max ?? 0;
      // Compare the SAME criteria in both rounds, not the totals.
      //
      // `max` moves between passes whenever a criterion becomes measurable or
      // stops being: an inconclusive criterion is subtracted from the
      // denominator, and whether a contention test concludes is genuinely
      // flaky ("2 of 6 concurrent clicks landed", "Page crashed").
      //
      // Rates were the previous attempt at this and are also wrong. A round
      // scored 49/50 and then 49/51 — the same 49 criteria passing, with one
      // extra criterion becoming measurable and failing. The rate fell from
      // 0.980 to 0.961, so it was called a regression, the app was rolled back,
      // and the level was lost. Nothing had got worse.
      //
      // Compare criteria that were conclusive in both rounds, but never let a
      // previous observation disappear: conclusive -> inconclusive is lost
      // evidence and rolls the source back instead of hiding a regression.
      const shared = compareCriterionEvidence(beforeBundle, bundle);
      if (shared.count === 0 && shared.lostEvidence.length === 0 && shared.definitionChanges.length === 0) {
        console.log('    no criteria were conclusively scored in both rounds; stopping');
        stalled = true;
        break;
      }
      if (shared.points < Math.min(beforeMax, afterMax)) {
        console.log(`    comparing ${shared.points} point(s) across ${shared.count} criteria scored in both rounds`
          + ` (${before}/${beforeMax} -> ${after}/${afterMax} overall)`);
      }
      const evidenceRegressed = shared.lostEvidence.length > 0 || shared.definitionChanges.length > 0;
      if (evidenceRegressed || shared.after < shared.before) {
        if (shared.lostEvidence.length) {
          console.log(`    lost conclusive evidence for ${shared.lostEvidence.length} criterion/criteria; rolling back and stopping`);
        } else if (shared.definitionChanges.length) {
          console.log('    rubric points changed between grades; rolling back and stopping');
        } else {
          console.log(`    regressed (${shared.before} -> ${shared.after} on shared criteria); rolling back and stopping`);
        }
        // Stop the servers BEFORE deleting what they are watching. Without
        // this, rolling back a regressed postgres run threw EBUSY on
        // app/server and took the whole finished run down with it.
        restoreSource(snapshot, appDir);
        bundle = grade(args, appDir, url, `${args.backend}-l${level}-rollback`, level, track, runId);
        regressed = true;
        stalled = true;
        break;
      }
      if (shared.after === shared.before) {
        console.log(`    no improvement (${shared.before} on shared criteria); stopping fix rounds`);
        stalled = true;
        break;
      }
    }

    // A grading run that crashed writes no bundle, and recording that as 0/0
    // makes a harness failure indistinguishable from an app that scored nothing
    // — in a ladder run it silently drops a level's result on the floor. Say so
    // instead, and leave the score null.
    const finalBundleOutcome = classifyBundle(bundle);
    const graded = !['ungraded', 'harness_failure'].includes(finalBundleOutcome.kind);
    if (!graded) {
      console.log(`  L${level}: GRADING DID NOT COMPLETE — no usable bundle. ` +
        `Score is unknown, not zero; re-grade this level before using the run.`);
    }
    const buildSession = { sessionId: build.sessionId, costUsd: build.costUsd,
      durationMs: build.durationMs, usage: build.usage ?? null,
      tokens: build.tokens ?? null, outputTokens: build.outputTokens ?? null,
      turns: build.turns ?? null, promptBytes: build.promptBytes ?? null,
      thinking: build.thinking ?? null, transcript: build.transcript ?? null,
      provenance: build.provenance ?? null, providerMetadata: build.providerMetadata ?? null };
    const sessionTotals = summarizeSessions([buildSession, ...fixSessions]);
    run.levels.push({
      level,
      graded,
      score: graded ? bundle.totals.score : null,
      max: graded ? bundle.totals.max : null,
      // Whether the guarantees earned at earlier levels still hold at this one —
      // the whole point of growing the app level by level. It reached the
      // console and the bundle but not run.json, so the thesis metric was
      // missing from the durable record.
      regression: bundle?.totals?.regression ?? null,
      selection: bundle?.selection ?? null,
      firstBuild,
      contractPass: bundle?.totals?.contractPass ?? null,
      code: bundle?.code ?? null,
      buildCostUsd: build.costUsd,
      fixCostUsd: Number(fixCost.toFixed(4)),
      buildSession,
      fixSessions,
      sessionTotals,
      tokens: sessionTotals.tokens,
      // Carried up so a run summary can explain a cost, not just report one.
      usage: sessionTotals.usage,
      turns: sessionTotals.turns,
      promptBytes: sessionTotals.promptBytes,
      tokensPerTurn: sessionTotals.turns
        ? Math.round(sessionTotals.tokens / sessionTotals.turns) : null,
      // Reasoning actually produced. The budget is deliberately unpinned so runs
      // measure what a customer gets; that is only defensible if a shift in the
      // CLI default is visible afterwards rather than silently absorbed into
      // every score. agent.mjs measured this from the session transcript and the
      // level record was dropping it, so the guarantee was not holding.
      thinking: sessionTotals.thinking,
      fixRounds,
      stalled,
      regressed,
      outcome: finalBundleOutcome,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    writeRunJson(join(args.out, 'run.json'), run);
  }

  if (args.mutations) {
    console.log(`\n================ ${args.backend} mutation control ================`);
    const pristineOutcome = aggregateRunOutcome(run.levels);
    if (mutationControlEligible(pristineOutcome)) {
      args.parentAttemptId = runId;
      run.mutationControl = runMutationControl(args, appDir, url, track);
    } else {
      console.log(`  skipped: pristine outcome is ${pristineOutcome.kind}`);
      run.mutationControl = { ok: false, skipped: true,
        outcome: { kind: pristineOutcome.kind, phase: 'mutation-control-prerequisite',
          reason: `pristine outcome is ${pristineOutcome.kind}` } };
    }
    writeRunJson(join(args.out, 'run.json'), run);
  }

  // Did the builds read the thing that grades them? Prevention has holes we
  // know about — permission rules do not govern a bash `cat` — so every run
  // audits its own transcripts and says so. A score nobody checked for this is
  // worth less than one that carries the check, and six runs were quoted for a
  // day before anyone looked.
  try {
    const audit = sh('node', [join(ROOT, 'leak-audit.mjs'), '--app', appDir, '--json'], { stdio: 'pipe' });
    const escapes = JSON.parse(audit).flatMap(r => r.hits ?? []);
    const serious = escapes.filter(h => /GRADER|CONTRACT|BENCHMARK NOTES|PROMPTS/.test(h.kind));
    run.contaminated = serious.length > 0;
    run.contamination = serious.length
      ? { evidence: [...new Set(serious.map(h => `${h.kind}: ${h.path.split('/').slice(-2).join('/')}`))].slice(0, 8),
          verdict: 'SCORES NOT USABLE — the build read the harness that grades it.' }
      : { evidence: 'no reads of the grader, contracts, prompts or notes', verdict: 'scores usable' };
    if (run.contaminated) {
      console.log(`\n  !! CONTAMINATED: this build read the harness that grades it —`);
      for (const e of run.contamination.evidence) console.log(`     ${e}`);
      console.log('     Scores from this run must not be quoted.');
    }
  } catch (e) {
    // An audit that could not run is not a pass. Treating "unknown" as usable is
    // how six contaminated runs got quoted for a day, so the unchecked case now
    // lands on the same side as the failed one.
    run.contaminated = true;
    run.contamination = { evidence: `audit did not run: ${String(e.message).split('\n')[0]}`,
      verdict: 'SCORES NOT USABLE — nothing verified this build stayed inside its directory.' };
    console.log('\n  !! AUDIT DID NOT RUN — scores from this run must not be quoted.');
  }

  // Keep the evidence. The transcripts the audit just read are pruned by the CLI
  // after 30 days, and that has already destroyed one benchmark's audit trail.
  try {
    sh('node', [join(ROOT, 'archive-transcripts.mjs'), '--app', appDir, '--label', artifactLabel],
      { stdio: 'pipe' });
  } catch { console.log('  (transcript archiving failed — evidence is on a 30-day timer)'); }

  run.totals = {
    // A level that never ran contributes nothing rather than NaN.
    score: run.levels.reduce((n, l) => n + (l.score ?? 0), 0),
    max: run.levels.reduce((n, l) => n + (l.max ?? 0), 0),
    costUsd: Number(run.levels.reduce((n, l) => n + (l.buildCostUsd ?? 0) + (l.fixCostUsd ?? 0), 0).toFixed(4)),
    fixRounds: run.levels.reduce((n, l) => n + (l.fixRounds ?? 0), 0),
    sessions: run.levels.reduce((n, l) => n + (l.sessionTotals?.sessions ?? 0), 0),
    tokens: run.levels.reduce((n, l) => n + (l.sessionTotals?.tokens ?? 0), 0),
    outputTokens: run.levels.reduce((n, l) => n + (l.sessionTotals?.outputTokens ?? 0), 0),
    turns: run.levels.reduce((n, l) => n + (l.sessionTotals?.turns ?? 0), 0),
    modelDurationMs: run.levels.reduce((n, l) => n + (l.sessionTotals?.durationMs ?? 0), 0),
    durationSec: Math.round((Date.now() - started) / 1000),
    // Which levels the totals are actually made of. A run missing a level is
    // not comparable with one that graded them all, and the summary has to
    // carry that rather than leaving it to be noticed.
    ungraded: run.levels.filter(l => !l.graded).map(l => l.level),
  };
  run.outcome = aggregateRunOutcome(run.levels);
  run.completedAt = new Date().toISOString();
  if (args.mutations && !run.mutationControl?.ok && !run.mutationControl?.skipped) {
    run.outcome = { kind: 'harness_failure', phase: 'mutation-control',
      reason: run.mutationControl?.outcome?.reason
        ?? run.mutationControl?.processError
        ?? 'one or more declared mutations were not cleanly caught',
      appFailures: [], inconclusive: [] };
  }
  writeRunJson(join(args.out, 'run.json'), run);

  // What the model fought with is the part SpacetimeDB can act on, and it is
  // only in the transcript — the score cannot say it. Appended to a running
  // file after every SpacetimeDB run so the pattern across runs is visible
  // rather than rediscovered each time.
  if (executeStackCapability(stackAdapter, 'run-policy', 'product-review-enabled')
    && run.setup?.session !== 'model-free-reference') {
    try {
      sh('node', [join(ROOT, 'stdb-report.mjs'), '--label', artifactLabel, '--track', args.track,
        '--level', String(args.levelList[args.levelList.length - 1]),
        '--score', `${run.totals.score}/${run.totals.max}`,
        '--cost', String(run.totals.costUsd),
        '--fix-rounds', String(run.totals.fixRounds),
        ...(run.contaminated ? ['--contaminated'] : [])], { stdio: 'inherit' });
    } catch (e) {
      console.log(`  (stdb friction report failed: ${String(e.message).split('\n')[0]})`);
    }
    // The counted errors are half the picture. The other half — repeated cycles,
    // workarounds, and API used wrongly but successfully — only shows in the
    // shape of what the model did, so the behavioural review runs too.
    try {
      sh('node', [join(ROOT, 'stdb-review.mjs'), '--label', artifactLabel,
        '--source', join(args.out, 'source'),
        '--compare', executeStackCapability(stackAdapter, 'run-policy', 'product-review-comparisons')
          .map(b => resultsName(track, b, args.runIndex)).join(',')], { stdio: 'inherit' });
    } catch (e) {
      console.log(`  (stdb behavioural review failed: ${String(e.message).split('\n')[0]})`);
    }
  }

  console.log(`\n================ ${args.backend} summary ================`);
  for (const l of run.levels) {
    const unaided = l.firstBuild?.score != null ? `${l.firstBuild.score}/${l.firstBuild.max} unaided → ` : '';
    const score = l.graded ? `${unaided}${l.score}/${l.max}` : 'NOT GRADED';
    console.log(`  L${l.level}: ${score}  ${l.fixRounds} fix round(s)  ` +
      `$${((l.buildCostUsd ?? 0) + (l.fixCostUsd ?? 0)).toFixed(2)}  ${l.durationSec}s`);
  }
  console.log(`  TOTAL ${run.totals.score}/${run.totals.max}  ` +
    `$${run.totals.costUsd}  ${run.totals.fixRounds} fix round(s)  ${run.totals.durationSec}s`);
  console.log(`  ${join(args.out, 'run.json')}`);

  // The built source is the evidence behind the score, and it only ever existed
  // in the work directory — results/ held run.json and nothing else. Copy it
  // back BEFORE the work directory goes away, or cleaning up destroys the thing
  // the run was for.
  try {
    snapshotSource(appDir, join(args.out, 'source'));
    console.log(`  source kept at ${join(args.out, 'source')}`);
  } catch (e) {
    console.log(`  !! could not keep the source: ${String(e.message).split('\n')[0]}`);
  }

  // bundle.json and the per-suite grading files say WHY each criterion failed,
  // and they lived only in the work directory — so cleaning up destroyed the
  // evidence behind the score. Asked why a contention criterion failed, the
  // answer was "the detail was deleted", which is no answer at all. Media is
  // skipped: traces and video are large and reproducible.
  try {
    const from = join(appDir, 'stack-bench');
    if (existsSync(from)) {
      cpSync(from, join(args.out, 'grading'), {
        recursive: true,
        filter: src => !/[\\/]media([\\/]|$)/.test(src),
      });
      console.log(`  grading detail kept at ${join(args.out, 'grading')}`);
    }
  } catch (e) {
    console.log(`  !! could not keep the grading detail: ${String(e.message).split('\n')[0]}`);
  }

  teardown();

  // Leave nothing in temp. Best-effort: a directory some process still holds is
  // not worth failing a finished run over, and the next run makes its own
  // anyway. Say so rather than leaving it to be discovered. Only for a
  // directory THIS run created — an explicit --app is the caller's.
  if (ownWorkDir) {
    try {
      rmSync(dirname(appDir), { recursive: true, force: true });
    } catch {
      console.log(`  (work dir still held: ${dirname(appDir)} — the next sweep will take it)`);
    }
  }
  process.exitCode = runExitCode(run.outcome);
}

main().catch(error => {
  console.error(error.stack ?? error.message);
  try { emergencyTeardown?.(); }
  catch (cleanupError) {
    console.error(`cleanup after failure also failed: ${String(cleanupError.message).split(/\r?\n/)[0]}`);
  }
  process.exitCode = 1;
});
