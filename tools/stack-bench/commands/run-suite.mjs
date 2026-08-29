#!/usr/bin/env node
// Stack Bench: grade one generated app end to end.
//
// Every manual step in this sequence has produced a wrong result at least once
// (grading a dirty database silently lowers scores; grading the wrong backend
// entirely when two apps collide on a port), so the sequence is automated and
// each precondition is verified rather than assumed.
//
//   stop hosted app -> reset database -> verify clean -> contract lint -> feature/invariant/delivery
//   suites -> bundle
//
// Usage:
//   node commands/run-suite.mjs --app <app-dir> --url <url> --backend spacetime|postgres|mongodb
//                      --label <id> [--out <dir>] [--media] [--level 1] [--no-reset]

import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { dbName, loadTrack, suitesFor, DEFAULT_TRACK } from '../src/composition/tracks.mjs';
import { answers as hostAnswers } from '../src/runtime/platform.mjs';
import { controlBackend } from '../src/runtime/backend-control.js';
import { readArtifactPayload, recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { bundleRecipeRelease, resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { createBoundRecipeTaskRequest, resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.mjs';
import { contractControlIds } from '../src/composition/agent-visible-contract.mjs';
import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.mjs';
import { criterionEvidence, evidencePassed } from '../src/evidence/check-evidence.js';
import { renderEvidenceConsoleLine } from '../src/evidence/evidence-presentation.js';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.mjs';
import { aggregatePackRuntime, exceededPackBudgets } from '../src/composition/pack-runtime.mjs';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.mjs';
import { readBackendLease } from '../src/runtime/backend-lease.mjs';
import { databaseContainerName } from '../src/stacks/database-containers.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { sha256 } from '../src/evidence/provenance.js';
import { GRADER_SOURCE_TIMEOUT_MS } from '../src/runtime/grading-timeout.js';

import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';
const RESET = join(ROOT, 'commands', 'reset-backend.mjs');

export function suitesForRecipe(track, binding) {
  if (!binding?.execution?.length) throw new Error('recipe has no typed execution plan');
  return binding.execution.map(entry => ({
    id: entry.id,
    spec: resolve(track.dir, entry.source),
    ...(entry.ownership.kind === 'inherited'
      ? { inherited: true, fromLevel: entry.ownership.fromLevel }
      : {}),
  }));
}

export function childFailureDetail(failure = null, stdout = '', limit = 600) {
  const processOutput = [failure?.stderr, stdout]
    .filter(value => value !== undefined && value !== null && String(value).trim())
    .join('\n').trim();
  const diagnostic = processOutput || String(failure?.message ?? '').trim();
  const lines = diagnostic.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  if (!lines.length) return '';
  const noise = line => line.startsWith('at ') || /^Node\.js v/.test(line)
    || /^node:internal\//.test(line) || /^\^+$/.test(line) || /^[\[\]{},]+$/.test(line);
  const cause = lines.find(line => !noise(line) && /(?:error|failed|timeout|closed|econn|killed)/i.test(line))
    ?? lines.find(line => !noise(line)) ?? lines[0];
  const selected = [cause, ...lines.slice(-4)].filter((line, index, all) => all.indexOf(line) === index);
  return selected.join(' | ').slice(0, limit);
}

export function resetFailureOutcome(error) {
  return error?.status === GENERATED_APP_LAYOUT_EXIT_CODE
    ? { kind: 'app_failure', phase: 'application-layout',
      appFailures: ['application-layout'] }
    : error?.code === 'generated_app_not_restartable'
    ? { kind: 'app_failure', phase: 'application-restart',
      appFailures: ['application-restart'] }
    : { kind: 'harness_failure', phase: 'database-reset' };
}

export function applicationFailureTotals(selection, declaredSuites) {
  if (!selection?.checks?.length) return {};
  const inherited = new Set(declaredSuites.filter(suite => suite.inherited).map(suite => suite.id));
  const currentMax = selection.checks.filter(check => !inherited.has(check.executionId))
    .reduce((total, check) => total + Number(check.points ?? 0), 0);
  const regressionMax = selection.checks.filter(check => inherited.has(check.executionId))
    .reduce((total, check) => total + Number(check.points ?? 0), 0);
  return { score: 0, max: currentMax, dirty: false, contractPass: null,
    regression: regressionMax ? { score: 0, max: regressionMax } : null };
}

export function clearPreviousGradeOutputs(output) {
  const generated = existsSync(output) ? readdirSync(output).filter(name =>
    /^grading-.+\.json$/.test(name) || /^grader-.+\.(?:stdout|stderr)\.log$/.test(name)) : [];
  for (const name of ['bundle.json', 'contract-lint.json', 'actions.json', 'media', 'failure-media',
    'database-provenance', ...generated]) {
    rmSync(join(output, name), { recursive: true, force: true });
  }
}

export function runGraderChild(argv, output, suiteId, { execute = spawnSync } = {}) {
  const options = { encoding: 'utf8', cwd: ROOT, timeout: COMMAND_TIMEOUT_MS,
    maxBuffer: 64 * 1024 * 1024 };
  const result = execute(process.execPath, argv, options);
  const stdout = redactCredentials(String(result.stdout ?? ''));
  const stderr = redactCredentials(String(result.stderr ?? ''));
  const safeId = String(suiteId).replace(/[^A-Za-z0-9._-]/g, '_');
  const stdoutName = `grader-${safeId}.stdout.log`;
  const stderrName = `grader-${safeId}.stderr.log`;
  writeFileSync(join(output, stdoutName), stdout);
  writeFileSync(join(output, stderrName), stderr);
  let failure = result.error ?? null;
  if (!failure && result.status !== 0) {
    failure = new Error(`grader exited ${result.status ?? result.signal ?? 'without status'}`);
  }
  if (failure) Object.assign(failure, { stdout, stderr, status: result.status, signal: result.signal });
  return { stdout, stderr, failure, stdoutName, stderrName };
}

export function databaseContainerForGrading(backend, env = process.env, {
  readLease = readBackendLease,
} = {}) {
  if (!['mongodb', 'postgres'].includes(backend)) return null;
  const lease = databaseLeaseForGrading(backend, env, { readLease });
  if (lease) return String(lease.resources.container.name);
  return databaseContainerName(backend, env);
}

export function databaseLeaseForGrading(backend, env = process.env, {
  readLease = readBackendLease,
} = {}) {
  if (!['mongodb', 'postgres'].includes(backend)) return null;
  const path = String(env.STACK_BENCH_LEASE ?? '').trim();
  const token = String(env.STACK_BENCH_LEASE_TOKEN ?? '').trim();
  if (!path && !token) return null;
  if (!path || !token) throw new Error('database grading requires both lease path and lease token');
  const lease = readLease(path, { token, backend, active: true });
  const container = String(lease.resources?.container?.name ?? '').trim();
  if (!container) throw new Error(`active ${backend} lease has no database container`);
  return lease;
}

export function databaseNameForGrading(track, runIndex, lease = null) {
  if (!lease) return dbName(track, runIndex);
  const database = String(lease.resources?.database ?? '').trim();
  if (!database) throw new Error('active database lease has no database name');
  return database;
}

function parseArgs(argv) {
  const a = { level: '1', reset: true, media: true, runIndex: 0, track: DEFAULT_TRACK,
    packIds: [], checkKeys: [], observation: 'scored' };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--label': a.label = argv[++i]; break;
      case '--out': a.out = argv[++i]; break;
      case '--level': a.level = argv[++i]; break;
      case '--recipe': a.recipe = argv[++i]; break;
      case '--recipe-task-json': a.recipeTask = JSON.parse(argv[++i]); break;
      case '--credential-aliases-json': a.credentialAliases = JSON.parse(argv[++i]); break;
      case '--regression-checks-json': a.regressionChecks = JSON.parse(argv[++i]); break;
      case '--observation': a.observation = argv[++i]; break;
      case '--source-sha256': a.sourceSha256 = argv[++i]; break;
      case '--no-media': a.media = false; break;
      case '--track': a.track = argv[++i]; break;
      case '--pack': a.packIds.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--check': a.checkKeys.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--reseed-probe': a.reseedProbe = argv[++i]; break;
      case '--reseed-probe-expectation-json': a.reseedProbeExpectation = JSON.parse(argv[++i]); break;
      case '--restart-cmd': a.restartCmd = argv[++i]; break;
      case '--restart-spec': a.restartSpec = JSON.parse(argv[++i]); break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--no-reset': a.reset = false; break;
      case '--parent-attempt-id': a.parentAttemptId = argv[++i]; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.app || !a.url || !a.backend || !a.label) {
    console.error('Usage: node commands/run-suite.mjs --app <dir> --url <url> --backend <b> --label <id> [--out <dir>] [--media] [--no-reset]');
    process.exit(2);
  }
  if (!['scored', 'observed'].includes(a.observation)) {
    throw new Error('--observation must be scored or observed');
  }
  if (a.observation === 'observed' && !/^[a-f0-9]{64}$/.test(a.sourceSha256 ?? '')) {
    throw new Error('observed specifications require --source-sha256');
  }
  if (a.sourceSha256 !== undefined && !/^[a-f0-9]{64}$/.test(a.sourceSha256)) {
    throw new Error('--source-sha256 must be a SHA-256 digest');
  }
  if (a.reseedProbeExpectation && !a.reseedProbe) {
    throw new Error('--reseed-probe-expectation-json requires --reseed-probe');
  }
  a.out ??= join(a.app, 'stack-bench');
  a.regressionChecks ??= [];
  if (!Array.isArray(a.regressionChecks)
    || a.regressionChecks.some(key => typeof key !== 'string' || !key)) {
    throw new Error('--regression-checks-json must contain stable check keys');
  }
  return a;
}

export function selectObservationScope(selectedTask, observation = 'scored') {
  const selection = selectedTask?.selection ?? null;
  if (observation === 'scored') return selection;
  if (observation !== 'observed') throw new Error(`unknown observation scope ${observation}`);
  if (selectedTask?.request?.schemaVersion !== 3 || !selection) {
    throw new Error('observed specifications require a modular schema-3 task request');
  }
  if (!selection.observedChecks.length) throw new Error('observed specification scope is empty');
  return {
    ...selection,
    observation: 'observed',
    checks: selection.observedChecks,
    scoredPoints: 0,
    observedPoints: selection.observedChecks.reduce((total, check) => total + check.points, 0),
  };
}

export function attachRegressionScope(selection, recipeBinding, declaredSuites, stableKeys = []) {
  if (!stableKeys.length) return selection;
  if (!selection || !recipeBinding?.release?.checkCatalog) {
    throw new Error('regression checks require a recipe-bound scored selection');
  }
  const uniqueKeys = [...new Set(stableKeys)];
  if (uniqueKeys.length !== stableKeys.length) throw new Error('regression checks contain duplicates');
  const currentKeys = new Set(selection.checks.map(check => check.stableKey));
  const catalog = new Map(recipeBinding.release.checkCatalog
    .map(check => [check.stableKey, check]));
  const inheritedSuites = new Set(declaredSuites.filter(suite => suite.inherited)
    .map(suite => suite.id));
  const regressionChecks = uniqueKeys.map(key => {
    if (currentKeys.has(key)) throw new Error(`regression check ${key} is already in the current score`);
    const check = catalog.get(key);
    if (!check) throw new Error(`regression check ${key} is absent from the cumulative recipe`);
    if (!inheritedSuites.has(check.executionId)) {
      throw new Error(`regression check ${key} does not belong to an inherited execution`);
    }
    return { ...check, treatment: check.treatment ?? 'regression' };
  });
  const evaluationDocument = { schemaVersion: 1, selectionSha256: selection.sha256,
    regressionChecks: uniqueKeys.slice().sort() };
  return {
    ...selection,
    checks: [...selection.checks, ...regressionChecks],
    regressionChecks,
    regressionPoints: regressionChecks.reduce((total, check) => total + check.points, 0),
    evaluationSha256: sha256(Buffer.from(canonicalDefinitionJson(evaluationDocument))),
  };
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const COMMAND_TIMEOUT_MS = GRADER_SOURCE_TIMEOUT_MS;
const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, {
    encoding: 'utf8', stdio: 'pipe', cwd: ROOT, timeout: COMMAND_TIMEOUT_MS, ...opts,
  });

// Does this URL respond at all? Any HTTP status counts — the question is whether
// a server is listening, not what it thinks of the request.
async function answers(url, attempts = 3) {
  for (let i = 0; i < attempts; i++) {
    if (hostAnswers(url, 4)) return true;
    await sleep(2000);
  }
  return false;
}

export async function verifyReseedProbe(url, expectation, {
  fetchImpl = fetch, timeoutMs = 5000,
} = {}) {
  let response;
  try {
    response = await fetchImpl(url, { signal: AbortSignal.timeout(timeoutMs) });
  } catch (error) {
    return { ok: false, count: null,
      detail: `startup data probe could not be read: ${error.message}` };
  }
  if (!response.ok) {
    return { ok: false, count: null,
      detail: `startup data probe returned HTTP ${response.status}` };
  }
  if (!expectation) return { ok: true, detail: null, count: null };
  let payload;
  try {
    payload = await response.json();
  } catch {
    return { ok: false, count: null, detail: 'startup data probe did not return JSON' };
  }
  const value = expectation.jsonPath.split('.').reduce((current, segment) =>
    current !== null && typeof current === 'object' ? current[segment] : undefined, payload);
  if (!Array.isArray(value)) {
    return { ok: false, count: null,
      detail: `startup data probe JSON path ${expectation.jsonPath} is not an array` };
  }
  if (value.length < expectation.minCount) {
    return { ok: false, count: value.length,
      detail: `startup data is missing: ${expectation.jsonPath} contains ${value.length} entries, `
        + `expected at least ${expectation.minCount}` };
  }
  return { ok: true, detail: null, count: value.length };
}

export async function waitForReseedProbe(url, expectation, {
  attempts = 9, intervalMs = 250, probeTimeoutMs = 1000,
  probe = verifyReseedProbe, sleepImpl = sleep,
} = {}) {
  if (!Number.isInteger(attempts) || attempts < 1) {
    throw new Error('reseed probe attempts must be a positive integer');
  }
  let result = null;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    result = await probe(url, expectation, { timeoutMs: probeTimeoutMs });
    if (result.ok || attempt === attempts) return result;
    await sleepImpl(intervalMs);
  }
  return result;
}

// The benchmark's own database containers. A generated app that connects
// somewhere else is not measuring what we think it is: one Postgres app pointed
// at an unrelated project's container on 5433 and graded "fine" while writing to
// a database the harness could not reset.
export function checkDatabaseProvenance(args) {
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const expected = executeStackCapability(adapter, 'ports', 'allocations').db;
  if (!expected) return { ok: true, reason: 'no external database for this backend' };
  // Under `--guidance minimal` the model chooses its own layout, so server/.env
  // is a prescribed-stack assumption. Hard-coding it aborted every minimal run
  // with "WRONG DATABASE" on apps whose connection string was simply somewhere
  // else. Search the app for the connection string instead of dictating where
  // it lives; the check is that the app targets the benchmark's database, not
  // that it stores the URL in a particular file.
  const urls = [];
  let usesLeasedEnvironment = false;
  const walk = dir => {
    if (!existsSync(dir)) return;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (/^(node_modules|dist|\.vite|\.git|module_bindings)$/.test(e.name)) continue;
      const p = join(dir, e.name);
      if (e.isDirectory()) { walk(p); continue; }
      if (!/\.(env|ts|tsx|js|mjs|json|yaml|yml)$|^\.env/.test(e.name)) continue;
      try {
        const text = readFileSync(p, 'utf8');
        if (/process\.env(?:\.DATABASE_URL|\[['"]DATABASE_URL['"]\])/.test(text)) {
          usesLeasedEnvironment = true;
        }
        const m = executeStackCapability(adapter, 'agent', 'find-database-urls', { text });
        if (m) urls.push(...m);
      } catch { /* unreadable file proves nothing */ }
    }
  };
  walk(args.app);
  if (usesLeasedEnvironment) {
    return { ok: true, url: 'process.env.DATABASE_URL',
      reason: 'app reads the database URL supplied by its authenticated backend lease' };
  }
  if (!urls.length) return { ok: false,
    reason: 'app neither reads process.env.DATABASE_URL nor contains a database connection string' };
  const matchesExpectedPort = value => {
    try { return Number(new URL(value).port) === Number(expected); }
    catch { return false; }
  };
  const ok = urls.some(matchesExpectedPort);
  return { ok, url: urls[0],
    reason: ok ? 'ok' : `app targets ${urls[0]} but the benchmark database is on port ${expected}` };
}

export function applicationDatabaseMarker(report, definition) {
  if (!report || !definition) return null;
  const markers = [];
  for (const feature of report.features ?? []) {
    for (const criterion of feature.criteria ?? []) {
      if (criterion.stableKey !== definition.check) continue;
      const action = criterionEvidence(criterion).actions.find(entry =>
        entry.evidence?.action?.id === definition.action
        && evidencePassed(entry.evidence));
      const marker = action?.evidence?.observation?.[definition.observationField];
      if (typeof marker === 'string' && marker) markers.push(marker);
    }
  }
  return new Set(markers).size === 1 ? markers[0] : null;
}

export function databaseProvenanceFailure(error) {
  return { kind: 'harness_failure', phase: 'database-provenance',
    reason: `runtime database provenance failed: ${error.message}` };
}

export function checkRuntimeDatabaseProvenance(args, marker = null) {
  if (!['mongodb', 'postgres'].includes(args.backend)) {
    return { ok: null, verified: false,
      reason: 'exact runtime database marker proof is not implemented for this stack' };
  }
  if (!args.databaseLease) {
    return { ok: null, verified: false,
      reason: 'standalone grading has no authenticated database lease' };
  }
  if (typeof marker !== 'string' || !marker) {
    return { ok: null, verified: false,
      reason: 'the application action did not produce a database marker' };
  }
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
    'database', 'prove-use', { lease: args.databaseLease, marker });
}

// Report the application size and direct runtime dependency count.
export function codeMetrics(args) {
  // Also a prescribed-stack assumption: under minimal guidance an app may put
  // its server anywhere, and a missing `server/` reported 0 LOC in 0 files
  // rather than admitting it had not found the code. Fall back to everything
  // outside the client when the conventional directory is absent.
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const conventional = executeStackCapability(adapter, 'agent', 'server-directory');
  const SERVER_DIR = existsSync(join(args.app, conventional)) ? conventional : '.';
  const walk = (dir, out = []) => {
    if (!existsSync(dir)) return out;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (/^(node_modules|dist|\.vite|module_bindings|drizzle)$/.test(e.name)) continue;
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p, out);
      // JavaScript counts too: a stack-free app is under no obligation to use
      // TypeScript, and one that wrote 17 .js and 10 .jsx files was reported as
      // "0 server LOC in 0 files" — a lie about the measurement, not a fact
      // about the app.
      else if (/\.(ts|tsx|js|jsx|mjs|cjs)$/.test(e.name)) out.push(p);
    }
    return out;
  };
  const count = files => files.reduce((n, f) => n + readFileSync(f, 'utf8').split('\n').length, 0);
  // With no conventional server directory, "server" is everything that is not
  // the client — otherwise the fallback counts the client twice and serverLoc
  // equals totalLoc, which reads as a much larger backend than was written.
  const allFiles = walk(args.app);
  const serverFiles = SERVER_DIR === '.'
    ? allFiles.filter(f => !/[\\/]client[\\/]/.test(f))
    : walk(join(args.app, SERVER_DIR));

  let deps = 0;
  const packageFiles = new Set([
    resolve(args.app, 'package.json'),
    resolve(args.app, SERVER_DIR, 'package.json'),
    resolve(args.app, 'client/package.json'),
  ]);
  for (const p of packageFiles) {
    if (!existsSync(p)) continue;
    try { deps += Object.keys(JSON.parse(readFileSync(p, 'utf8')).dependencies ?? {}).length; } catch { /* ignore */ }
  }

  return {
    serverLoc: count(serverFiles), serverFiles: serverFiles.length,
    totalLoc: count(allFiles), totalFiles: allFiles.length,
    runtimeDeps: deps,
  };
}

export function findMutationBackups(app, { readDir = readdirSync } = {}) {
  const backups = [];
  const walk = dir => {
    let entries;
    try {
      entries = readDir(dir, { withFileTypes: true });
    } catch (error) {
      // Vite atomically replaces transient dependency directories while the
      // app runs. They are not source and may vanish between parent and child
      // reads; a missing directory cannot contain a mutation backup.
      if (error?.code === 'ENOENT') return;
      throw error;
    }
    for (const entry of entries) {
      if (/^(node_modules|dist|\.vite|\.git|module_bindings)$/.test(entry.name)) continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name.endsWith('.mutation-backup')) backups.push(path);
    }
  };
  walk(app);
  return backups;
}

function resetDatabase(args) {
  process.stdout.write('  reset database ... ');
  try {
    run(process.execPath, [RESET, args.backend, args.app]);
    console.log('ok');
  } catch (err) {
    console.log('FAILED');
    const detail = childFailureDetail(err, err?.stdout);
    console.log(`    ${detail}`);
    return { ok: false, detail, outcome: resetFailureOutcome(err) };
  }
  return { ok: true, detail: null, outcome: null };
}

export function contractLintArgv(args, selectedTask = null) {
  const controls = selectedTask ? contractControlIds(selectedTask.task.contractText) : [];
  const out = join(args.out, 'contract-lint.json');
  return [join(ROOT, 'linter', 'lint.mjs'), '--url', args.url, '--level', args.level,
      '--track', args.track, '--label', args.label, '--out', out,
      '--parent-attempt-id', args.bundleArtifactId,
      ...(args.credentialAliases
        ? ['--credential-aliases-json', JSON.stringify(args.credentialAliases)] : []),
      ...controls.flatMap(id => ['--hook', id])];
}

function lint(args, selectedTask = null) {
  process.stdout.write('  contract lint ... ');
  const out = join(args.out, 'contract-lint.json');
  rmSync(out, { force: true });
  try {
    run('node', contractLintArgv(args, selectedTask));
  } catch { /* non-zero exit means hooks failed; the report still lands */ }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = readArtifactPayload(out, { expectedKind: 'contract_lint' });
  console.log(r.pass
    ? `PASS (${r.counts.pass} hooks)`
    : `FAIL (${r.counts.fail} failed, ${r.counts.blocked} blocked)`);
  return r;
}

// Named write actions let concurrency checks issue authenticated operations
// without prescribing one transport. Missing actions are reported explicitly.
function checkActions(args) {
  process.stdout.write(`  ${'actions'.padEnd(10)} ... `);
  const out = join(args.out, 'actions.json');
  rmSync(out, { force: true });
  try {
    run('node', [join(ROOT, 'commands', 'check-actions.mjs'), '--backend', args.backend,
      '--url', args.url, '--app', args.app ?? '.', '--track', args.track, '--out', out, '--quiet',
      '--parent-attempt-id', args.bundleArtifactId]);
  } catch { /* non-zero exit means something is missing; the report still lands */ }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = readArtifactPayload(out, { expectedKind: 'action_check' });
  if (!r.missing.length) { console.log(`all ${r.results.length} present`); return r; }
  console.log(`${r.missing.length} MISSING — ${r.missing.join(', ')}`);
  console.log('             contention and volume criteria cannot be issued against this app,');
  console.log('             and will be excluded rather than failed.');
  return r;
}

function gradeSuite(args, suite, track, recipeBinding, bundleArtifactId, selectedChecks = [],
  { recordSelection = true, captureMedia = true, outputDirectory = args.out } = {}) {
  process.stdout.write(`  ${suite.id.padEnd(10)} ... `);
  mkdirSync(outputDirectory, { recursive: true });
  const out = join(outputDirectory, `grading-${suite.id}.json`);
  rmSync(out, { force: true });
  const argv = [join(ROOT, 'grader', 'grade.mjs'), '--url', args.url, '--level', args.level,
    '--label', `${args.label}-${suite.id}`, '--out', out];
  if (suite.spec) argv.push('--spec', suite.spec);
  argv.push('--backend', args.backend, '--track', args.track);
  if (recipeBinding) argv.push('--expected-recipe-sha256', recipeBinding.release.contentSha256);
  const requestedRecipe = args.recipe ?? (args.recipeTask
    ? `${args.recipeTask.recipe.id}@${args.recipeTask.recipe.version}` : null);
  if (requestedRecipe) argv.push('--recipe', requestedRecipe);
  for (const check of selectedChecks) argv.push('--selected-check', check.stableKey);
  if (args.credentialAliases) {
    argv.push('--credential-aliases-json', JSON.stringify(args.credentialAliases));
  }
  if (recordSelection && args.selection?.sha256) {
    argv.push('--selection-sha256', args.selection.evaluationSha256 ?? args.selection.sha256);
  }
  argv.push('--parent-attempt-id', bundleArtifactId);
  // The out-of-band write goes straight to this run's database, with no
  // app code in the loop; only the harness knows which one that is.
  argv.push('--db-name', databaseNameForGrading(track, args.runIndex ?? 0, args.databaseLease));
  if (args.databaseContainer) argv.push('--database-container', args.databaseContainer);
  if (args.restartSpec) argv.push('--restart-spec', JSON.stringify(args.restartSpec));
  else if (args.restartCmd) argv.push('--restart-cmd', args.restartCmd);
  // The systems criteria run scripts the app itself ships (back-office writes),
  // so the grader has to know where the app lives.
  if (args.app) argv.push('--app', args.app);
  if (captureMedia && args.media) argv.push('--media', join(outputDirectory, 'media'), '--trace');
  else if (captureMedia) argv.push('--failure-media', join(outputDirectory, 'failure-media'));
  const child = runGraderChild(argv, outputDirectory, suite.id);
  const { stdout, failure } = child;
  if (!existsSync(out)) {
    console.log('NO REPORT');
    const detail = childFailureDetail(failure, stdout);
    throw new Error(`grader produced no report for ${suite.id}${detail ? `: ${detail}` : ''}; `
      + `full diagnostics: ${child.stdoutName}, ${child.stderrName}`);
  }
  const r = readArtifactPayload(out, { expectedKind: 'grade' });
  if (selectedChecks.length) {
    const expected = selectedChecks.map(check => check.stableKey).sort();
    const reported = (r.selection?.checks ?? []).map(check => check.stableKey).sort();
    if (JSON.stringify(reported) !== JSON.stringify(expected)) {
      throw new Error(`grader report scope differs from requested suite scope for ${suite.id}`);
    }
  }
  console.log(`${r.total}/${r.max}`);
  for (const f of r.features) {
    for (const c of f.criteria.filter(c => !evidencePassed(criterionEvidence(c)))) {
      console.log(`      ${renderEvidenceConsoleLine(criterionEvidence(c), `${f.name} / ${c.id}`, {
        includeSummary: false,
      })}`);
    }
  }
  // A criterion that PASSED on interface behaviour alone, because its
  // server-side check could not be run against this backend, is a weaker result
  // than one where the server refused a real request. Saying so on every run is
  // the difference between a disclosed limitation and a flattering score.
  const uiOnly = r.features.flatMap(f =>
    f.criteria.filter(c => evidencePassed(criterionEvidence(c)) && c.serverCheck === 'unverified')
      .map(c => `${f.name}/${c.id}`));
  if (uiOnly.length) {
    console.log(`      note: ${uiOnly.length} criterion/criteria passed on interface behaviour only`);
    for (const u of uiOnly) console.log(`            ${u} — server-side check not runnable on this backend`);
  }
  return r;
}

function databaseProvenanceChecks(definition, recipeBinding) {
  if (!definition || !recipeBinding) return [];
  const check = recipeBinding.release.checkCatalog.find(candidate =>
    candidate.stableKey === definition.check);
  if (!check) {
    throw new Error(`database provenance check is absent from the bound recipe: ${definition.check}`);
  }
  return [check];
}

async function main() {
  const startedAt = new Date().toISOString();
  const args = parseArgs(process.argv);
  args.databaseLease = databaseLeaseForGrading(args.backend);
  args.databaseContainer = args.databaseLease?.resources.container.name
    ?? (['mongodb', 'postgres'].includes(args.backend) ? databaseContainerName(args.backend) : null);
  const track = loadTrack(args.track);
  const recipeBinding = resolveRecipeRelease(track, args.level, args.recipeTask?.recipe ?? args.recipe);
  if (!recipeBinding && (args.packIds.length || args.checkKeys.length)) {
    throw new Error('--pack and --check require a recipe-bound level');
  }
  const selectedTask = recipeBinding
    ? (args.recipeTask
        ? resolveBoundRecipeTaskRequest(recipeBinding, args.recipeTask)
        : createBoundRecipeTaskRequest(recipeBinding, args))
    : null;
  let selection = selectObservationScope(selectedTask, args.observation);
  if (args.sourceSha256) {
    const source = hashAppSource(args.app);
    if (source.sha256 !== args.sourceSha256) {
      throw new Error('live application source differs from the source selected for grading');
    }
  }
  args.selection = selection;
  const declaredSuites = recipeBinding
    ? suitesForRecipe(track, recipeBinding)
    : suitesFor(track, args.level);
  if (args.observation === 'scored') {
    selection = attachRegressionScope(selection, recipeBinding, declaredSuites,
      args.regressionChecks);
  } else if (args.regressionChecks.length) {
    throw new Error('observed grading cannot include regression checks');
  }
  if (selection) {
    const suiteIds = new Set(declaredSuites.map(suite => suite.id));
    const unmapped = selection.checks.filter(check => !suiteIds.has(check.executionId));
    if (unmapped.length) {
      throw new Error(`selected recipe checks do not map to a declared suite: ${
        unmapped.map(check => check.stableKey).join(', ')}`);
    }
  }
  const calibration = resolveCalibrationForRelease(recipeBinding?.release ?? null, {
    trackRoot: track.dir,
    stackBenchRoot: ROOT,
  });
  const observationSuffix = args.observation === 'observed' ? '-observed' : '';
  const bundleArtifactId = `${args.parentAttemptId ?? args.label}-grade-bundle-l${args.level}${observationSuffix}`;
  args.bundleArtifactId = bundleArtifactId;
  mkdirSync(args.out, { recursive: true });
  // A structural abort can happen before any suite overwrites its old result.
  // Remove all outputs from the prior grade. A cumulative level can rename
  // inherited suites, so deleting only the current names leaves stale L1
  // evidence in an L2 result package.
  clearPreviousGradeOutputs(args.out);

  console.log(`\n=== ${args.label} (${args.backend}) ===`);
  console.log(`  app: ${args.app}`);
  console.log(`  url: ${args.url}`);
  if (recipeBinding) {
    console.log(`  recipe: ${recipeBinding.alias} -> ${recipeBinding.release.id}@${recipeBinding.release.version} ` +
      `(${recipeBinding.status}, ${recipeBinding.release.contentSha256.slice(0, 12)})`);
    console.log(args.observation === 'observed'
      ? `  scope: ${selection.checks.length} observed check(s), ${selection.observedPoints} observed point(s), 0 score contribution`
      : `  scope: ${selection.checks.length} check(s), ${selection.scoredPoints} point(s)`);
    if (selection.requested.packs?.length) console.log(`    packs: ${selection.requested.packs.join(', ')}`);
    if (selection.requested.features?.length) {
      console.log(`    features: ${selection.requested.features.join(', ')}`);
    }
    if (selection.requested.checks.length) console.log(`    extra checks: ${selection.requested.checks.join(', ')}`);
  }

  const bundle = {
    definitionSchemaVersion: track.schemaVersion,
    recipeRelease: bundleRecipeRelease(recipeBinding),
    calibration: calibration ? { id: calibration.id, version: calibration.version,
      state: calibration.state, contentSha256: calibration.contentSha256 } : null,
    label: args.label, track: args.track, backend: args.backend, url: args.url, app: args.app,
    level: Number(args.level), observation: args.observation,
    ...(args.sourceSha256 ? { source: { sha256: args.sourceSha256 } } : {}),
    suites: {}, totals: {},
    selection: selection ? { ...selection, attemptedChecks: [], reportedChecks: [], notRun: [] } : null,
  };
  const selectedPackIds = new Set(selection?.checks.map(check => check.packId) ?? []);
  const selectedPackDefinitions = recipeBinding?.plan.packs
    .filter(pack => selectedPackIds.has(pack.id)) ?? [];
  const writeBundle = () => {
    if (args.sourceSha256) {
      const current = hashAppSource(args.app);
      if (current.sha256 !== args.sourceSha256) {
        bundle.error = 'application source changed while grading was in progress';
        bundle.outcome = { kind: 'harness_failure', phase: 'source-provenance',
          reason: bundle.error };
      }
    }
    return writeArtifact(join(args.out, 'bundle.json'), {
      kind: 'grade_bundle',
      id: bundleArtifactId,
      attempt: { id: bundleArtifactId, parentId: args.parentAttemptId ?? null },
      timestamps: { startedAt, completedAt: new Date().toISOString() },
      identities: recipeArtifactIdentities(recipeBinding?.release ?? null, {
        calibration: calibration ? { id: calibration.id, version: calibration.version,
          sha256: calibration.contentSha256, state: calibration.state } : null,
        stackAdapter: { id: args.backend },
      }),
      payload: bundle,
    });
  };
  const recordApplicationAbort = () => {
    bundle.totals = applicationFailureTotals(selection, declaredSuites);
  };
  const freshenFailureMessage = () => lastResetOutcome?.phase === 'application-seed'
    ? `application startup seeding failed after database reset${lastResetFailure ? `: ${lastResetFailure}` : ''}`
    : `database reset failed — scores would not be comparable${lastResetFailure ? `: ${lastResetFailure}` : ''}`;
  const markRemainingNotRun = reason => {
    if (!bundle.selection) return;
    const accounted = new Set([
      ...bundle.selection.attemptedChecks,
      ...bundle.selection.notRun.map(check => check.stableKey),
    ]);
    bundle.selection.notRun.push(...bundle.selection.checks
      .filter(check => !accounted.has(check.stableKey))
      .map(check => ({ stableKey: check.stableKey, reason })));
  };

  // Reset before EVERY step, not once per run: the lint and each suite create
  // state of their own, so a single up-front reset leaves later suites grading
  // dirty state — which silently lowers scores.
  let lastResetFailure = null;
  let lastResetOutcome = { kind: 'harness_failure', phase: 'database-reset' };
  const freshen = async () => {
    if (!args.reset) return true;
    const requiresReseed = executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
      'reset', 'requires-reseed');
    const controlledRestart = track.reseedOnReset && Boolean(args.restartSpec) && requiresReseed;
    if (controlledRestart) {
      process.stdout.write('  stop application ... ');
      try {
        await controlBackend(args.restartSpec, 'stop');
        console.log('ok');
      } catch (error) {
        lastResetFailure = childFailureDetail(error);
        lastResetOutcome = { kind: 'harness_failure', phase: 'application-reset-control' };
        console.log(`FAILED (${lastResetFailure})`);
        return false;
      }
    }
    const reset = resetDatabase(args);
    lastResetFailure = reset.detail;
    lastResetOutcome = reset.outcome ?? { kind: 'harness_failure', phase: 'database-reset' };
    if (!reset.ok) return false;
    // Do not grade until the reset application is reachable. Hosted backends
    // can also require their startup data to be present again.
    const waitUntilReady = async () => {
      const seeded = await waitForReseedProbe(args.reseedProbe ?? args.url,
        args.reseedProbeExpectation);
      if (!seeded.ok) {
        lastResetFailure = seeded.detail;
        lastResetOutcome = { kind: 'app_failure', phase: 'application-seed',
          appFailures: ['application-seed'] };
        console.log(`FAILED (${seeded.detail})`);
        return false;
      }
      console.log(seeded.count === null ? 'ok' : `ok (${seeded.count} entries observed)`);
      return true;
    };
    if (track.reseedOnReset && (args.restartSpec || args.restartCmd) && requiresReseed) {
      process.stdout.write('  reseed      ... ');
      // Judge restart success with the readiness probe. The restart command can
      // leave a long-running server process behind, so the command also needs a deadline.
      try {
        // Do not give a background server an inherited pipe that keeps the
        // synchronous restart command open.
        if (args.restartSpec) {
          await controlBackend(args.restartSpec, controlledRestart ? 'start' : 'restart');
        }
        else run('bash', ['-c', args.restartCmd], { stdio: 'ignore', timeout: 200_000 });
      } catch (err) {
        lastResetOutcome = resetFailureOutcome(err);
        if (args.reseedProbe && await answers(args.reseedProbe)) {
          // A background server can keep the restart command's stdio open.
          // The live probe is authoritative, but seed validation below still
          // has to pass before grading may use the application.
        }
        else {
          // Say what actually went wrong. A bare "did not come back" sent the
          // first investigation looking at the application, when the fault was
          // the command line the harness built.
          const detail = ((err.stderr || '') + (err.stdout || '') + (err.message || ''))
            .toString().trim().split('\n').slice(-3).join(' | ').slice(0, 300);
          lastResetFailure = detail || null;
          console.log('FAILED (server did not come back)');
          console.log(`    control: ${args.restartSpec ? JSON.stringify(args.restartSpec) : args.restartCmd}`);
          console.log(`    ${detail}`);
          return false;
        }
      }

      return await waitUntilReady();
    }
    process.stdout.write('  ready       ... ');
    return await waitUntilReady();
  };

  bundle.code = codeMetrics(args);
  console.log(`  code        ... ${bundle.code.serverLoc} server LOC in ${bundle.code.serverFiles} files, ` +
    `${bundle.code.totalLoc} total LOC, ${bundle.code.runtimeDeps} runtime deps`);

  // An interrupted mutation run leaves the app deliberately broken with a
  // backup beside it. Grading that produces confident numbers for source
  // nobody intended to measure.
  const mutated = findMutationBackups(args.app);
  if (mutated.length) {
    bundle.error = `app still carries mutation backups (${mutated.join(', ')}) — its source is mutated, not the build under test`;
    bundle.outcome = { kind: 'harness_failure', phase: 'mutation-cleanup', reason: bundle.error };
    markRemainingNotRun('run aborted because application source is still mutated');
    writeBundle();
    console.log(`\nABORTED: ${bundle.error}`);
    process.exit(1);
  }

  const prov = checkDatabaseProvenance(args);
  bundle.provenance = prov;
  console.log(`  database    ... ${prov.ok ? 'benchmark-owned' : `WRONG DATABASE — ${prov.reason}`}`);
  if (!prov.ok) {
    bundle.error = `app is not using the benchmark database: ${prov.reason}`;
    bundle.outcome = { kind: 'app_failure', phase: 'database-provenance', reason: bundle.error,
      appFailures: ['database-provenance'] };
    recordApplicationAbort();
    markRemainingNotRun('run aborted because database provenance was invalid');
    writeBundle();
    console.log('\nABORTED: results would not describe the benchmark environment.');
    process.exit(1);
  }

  if (args.observation === 'scored') {
    if (!(await freshen())) {
      bundle.error = freshenFailureMessage();
      bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
      if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
      markRemainingNotRun(`run aborted: ${bundle.error}`);
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    let runtime = checkRuntimeDatabaseProvenance(args);
    let proofError = null;
    const proof = track.databaseProvenance;
    const requiresRuntimeProof = ['mongodb', 'postgres'].includes(args.backend)
      && args.databaseLease && args.reset;
    if (requiresRuntimeProof && !proof) {
      proofError = new Error(`${args.track} does not define a runtime database provenance check`);
    } else if (requiresRuntimeProof) {
      try {
        const checks = databaseProvenanceChecks(proof, recipeBinding);
        const report = gradeSuite(args, { id: 'database-provenance', spec: proof.scenario },
          track, recipeBinding, bundleArtifactId, checks,
          { recordSelection: false, captureMedia: false,
            outputDirectory: join(args.out, 'database-provenance') });
        const marker = applicationDatabaseMarker(report, proof);
        runtime = marker
          ? checkRuntimeDatabaseProvenance(args, marker)
          : { ok: false, verified: false,
              reason: 'the configured application action did not produce one database marker' };
      } catch (error) {
        proofError = error;
      }

      // The proof writes unique data through the application. Remove it before
      // linting and scored grading so the proof cannot change the result.
      if (!(await freshen())) {
        bundle.error = freshenFailureMessage();
        bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
        if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
        markRemainingNotRun(`run aborted: ${bundle.error}`);
        writeBundle();
        console.log(`\nABORTED: ${bundle.error}`);
        process.exit(1);
      }
    } else if (['mongodb', 'postgres'].includes(args.backend) && !args.reset) {
      runtime = { ok: null, verified: false,
        reason: 'runtime marker proof requires database reset to isolate its write' };
    }

    if (proofError) {
      bundle.outcome = databaseProvenanceFailure(proofError);
      bundle.error = bundle.outcome.reason;
      markRemainingNotRun('run aborted because runtime database provenance could not be verified');
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }

    bundle.provenance.runtime = runtime;
    console.log(`  db runtime  ... ${runtime.verified
      ? runtime.ok ? runtime.reason : `WRONG DATABASE — ${runtime.reason}`
      : runtime.reason}`);
    if (runtime.ok === false) {
      bundle.error = `app did not write its marker to the benchmark database: ${runtime.reason}`;
      bundle.outcome = { kind: 'app_failure', phase: 'database-provenance', reason: bundle.error,
        appFailures: ['database-provenance'] };
      recordApplicationAbort();
      markRemainingNotRun('run aborted because runtime database provenance failed');
      writeBundle();
      console.log('\nABORTED: application data came from outside the benchmark database.');
      process.exit(1);
    }
    bundle.suites.lint = lint(args, selectedTask);
    bundle.actions = checkActions(args);
  }

  // Two numbers, kept apart on purpose. `score` is this level's own work.
  // `regression` is whether the guarantees earned at earlier levels still hold.
  // Summing them would hide the finding: an app that adds every L3 feature and
  // silently breaks live stock updates from L1 would still read as progress.
  let total = 0, max = 0, regTotal = 0, regMax = 0, dirty = false;
  for (const suite of declaredSuites) {
    const selectedChecks = selection?.checks.filter(check => check.executionId === suite.id) ?? [];
    if (selection && selectedChecks.length === 0) {
      console.log(`  ${suite.id.padEnd(10)} ... not selected`);
      continue;
    }
    if (!(await freshen())) {
      bundle.error = freshenFailureMessage();
      console.log(`  ${suite.id}: SKIPPED (${bundle.error})`);
      markRemainingNotRun(`run aborted: ${bundle.error}`);
      bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
      if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    if (bundle.selection) {
      bundle.selection.attemptedChecks.push(...selectedChecks.map(check => check.stableKey));
    }
    let r;
    try {
      r = gradeSuite(args, suite, track, recipeBinding, bundleArtifactId, selectedChecks);
    } catch (error) {
      markRemainingNotRun(`run aborted after ${suite.id} grader failure`);
      bundle.error = error.message;
      bundle.outcome = { kind: 'harness_failure', phase: `grade:${suite.id}`, reason: bundle.error };
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    bundle.suites[suite.id] = r;
    if (bundle.selection) bundle.selection.reportedChecks.push(...selectedChecks.map(check => check.stableKey));
    if (selection) {
      bundle.packRuntime = aggregatePackRuntime(
        Object.values(bundle.suites).filter(candidate => candidate?.packRuntime),
        selectedPackDefinitions);
      const exceeded = exceededPackBudgets(bundle.packRuntime);
      if (exceeded.length) {
        // Runtime budgets qualify the benchmark's known-good references; they
        // are not a deadline for generated applications. A broken app can
        // legitimately consume several assertion timeouts in one pack. Keep
        // grading so it receives a complete repair report, while retaining the
        // exceeded measurement for diagnostics and qualification policy.
        console.log(`  runtime    ... ${exceeded.map(pack =>
          `${pack.id} ${pack.measuredRuntimeMs}ms > ${pack.budget.maxRuntimeMs}ms`)
          .join(', ')} [recorded; grading continues]`);
      }
    }
    if (suite.inherited) { regTotal += r.total; regMax += r.max; }
    else { total += r.total; max += r.max; }
  }

  bundle.totals = {
    score: total, max, dirty, contractPass: bundle.suites.lint?.pass ?? null,
    // null rather than 0/0 at L1, where there is nothing earlier to regress.
    regression: regMax ? { score: regTotal, max: regMax } : null,
  };
  writeBundle();

  console.log(`  ${'TOTAL'.padEnd(10)} ... ${total}/${max}${dirty ? '  [DIRTY]' : ''}`);
  if (regMax) {
    const kept = regTotal === regMax ? 'all earlier guarantees still hold' : `${regMax - regTotal} EARLIER GUARANTEE(S) LOST`;
    console.log(`  ${'REGRESSION'.padEnd(10)} ... ${regTotal}/${regMax}  — ${kept}`);
  }
  console.log(`  bundle: ${join(args.out, 'bundle.json')}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
