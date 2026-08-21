#!/usr/bin/env node
// Stack Bench: grade one generated app end to end.
//
// Every manual step in this sequence has produced a wrong result at least once
// (grading a dirty database silently lowers scores; grading the wrong backend
// entirely when two apps collide on a port), so the sequence is automated and
// each precondition is verified rather than assumed.
//
//   reset database -> verify clean -> contract lint -> feature/invariant/delivery
//   suites -> bundle
//
// Usage:
//   node commands/run-suite.mjs --app <app-dir> --url <url> --backend spacetime|postgres|mongodb
//                      --label <id> [--out <dir>] [--media] [--level 1] [--no-reset]

import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { loadTrack, suitesFor, DEFAULT_TRACK } from '../src/composition/tracks.mjs';
import { answers as hostAnswers } from '../src/runtime/platform.mjs';
import { controlBackend } from '../src/runtime/backend-control.mjs';
import { readArtifactPayload, recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.mjs';
import { bundleRecipeRelease, resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { createBoundRecipeTaskRequest, resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.mjs';
import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.mjs';
import { criterionEvidence, evidencePassed } from '../src/evidence/check-evidence.mjs';
import { renderEvidenceConsoleLine } from '../src/evidence/evidence-presentation.mjs';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.mjs';
import { aggregatePackRuntime, exceededPackBudgets } from '../src/composition/pack-runtime.mjs';
import { hashAppSource } from '../src/runtime/source-snapshot.mjs';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.mjs';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';
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
  const lines = [failure?.stderr, stdout, failure?.message]
    .filter(value => value !== undefined && value !== null && String(value).trim())
    .join('\n').trim().split(/\r?\n/).map(line => line.trim()).filter(Boolean);
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

export function clearPreviousGradeOutputs(output, declaredSuites) {
  for (const name of ['bundle.json', 'contract-lint.json', 'actions.json', 'media', 'failure-media',
    ...declaredSuites.flatMap(suite => [`grading-${suite.id}.json`,
      `grader-${suite.id}.stdout.log`, `grader-${suite.id}.stderr.log`])]) {
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
  if (a.observation === 'scored' && a.sourceSha256 !== undefined) {
    throw new Error('--source-sha256 is reserved for observed specifications');
  }
  if (a.reseedProbeExpectation && !a.reseedProbe) {
    throw new Error('--reseed-probe-expectation-json requires --reseed-probe');
  }
  a.out ??= join(a.app, 'stack-bench');
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

const sleep = ms => new Promise(r => setTimeout(r, ms));
const COMMAND_TIMEOUT_MS = 15 * 60_000;
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

export async function verifyReseedProbe(url, expectation, { fetchImpl = fetch } = {}) {
  if (!expectation) return { ok: true, detail: null, count: null };
  let response;
  try {
    response = await fetchImpl(url, { signal: AbortSignal.timeout(5000) });
  } catch (error) {
    return { ok: false, count: null,
      detail: `startup data probe could not be read: ${error.message}` };
  }
  if (!response.ok) {
    return { ok: false, count: null,
      detail: `startup data probe returned HTTP ${response.status}` };
  }
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
  const ok = urls.some(u => u.includes(`:${expected}/`));
  return { ok, url: urls[0],
    reason: ok ? 'ok' : `app targets ${urls[0]} but the benchmark database is on port ${expected}` };
}

// Same features for less code is a structural property of the platform, not a
// property of the model that happened to write it — unlike build cost, which
// inverted between Sonnet 4.6 and Sonnet 5.
function codeMetrics(args) {
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
  for (const pj of ['package.json', join(SERVER_DIR, 'package.json'), 'client/package.json']) {
    const p = join(args.app, pj);
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

function lint(args) {
  process.stdout.write('  contract lint ... ');
  const out = join(args.out, 'contract-lint.json');
  rmSync(out, { force: true });
  try {
    run('node', [join(ROOT, 'linter', 'lint.mjs'), '--url', args.url, '--level', args.level,
      '--track', args.track, '--label', args.label, '--out', out,
      '--parent-attempt-id', args.bundleArtifactId]);
  } catch { /* non-zero exit means hooks failed; the report still lands */ }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = readArtifactPayload(out, { expectedKind: 'contract_lint' });
  console.log(r.pass ? `PASS (${r.counts.pass} hooks)` : `FAIL (${r.counts.fail + r.counts.blocked} missing)`);
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

function gradeSuite(args, suite, track, recipeBinding, bundleArtifactId, selectedChecks = []) {
  process.stdout.write(`  ${suite.id.padEnd(10)} ... `);
  const out = join(args.out, `grading-${suite.id}.json`);
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
  if (args.selection?.sha256) argv.push('--selection-sha256', args.selection.sha256);
  argv.push('--parent-attempt-id', bundleArtifactId);
  // The out-of-band write goes straight to this run's database, with no
  // app code in the loop; only the harness knows which one that is.
  argv.push('--db-name', `stackbench${track.slug ? '_' + track.slug : ''}_run${args.runIndex ?? 0}`);
  if (args.restartSpec) argv.push('--restart-spec', JSON.stringify(args.restartSpec));
  else if (args.restartCmd) argv.push('--restart-cmd', args.restartCmd);
  // The systems criteria run scripts the app itself ships (back-office writes),
  // so the grader has to know where the app lives.
  if (args.app) argv.push('--app', args.app);
  if (args.media) argv.push('--media', join(args.out, 'media'), '--trace');
  else argv.push('--failure-media', join(args.out, 'failure-media'));
  const child = runGraderChild(argv, args.out, suite.id);
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
  const dirty = r.environment?.preexistingRooms > 0 ? r.environment.preexistingRooms : 0;
  console.log(`${r.total}/${r.max}${dirty ? `  [DIRTY: ${dirty} rooms — not comparable]` : ''}`);
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

async function main() {
  const startedAt = new Date().toISOString();
  const args = parseArgs(process.argv);
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
  const selection = selectObservationScope(selectedTask, args.observation);
  if (args.observation === 'observed') {
    const source = hashAppSource(args.app);
    if (source.sha256 !== args.sourceSha256) {
      throw new Error('live application source changed after the bound first-build snapshot');
    }
  }
  args.selection = selection;
  const declaredSuites = recipeBinding
    ? suitesForRecipe(track, recipeBinding)
    : suitesFor(track, args.level);
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
  // Remove only the exact outputs this grading pass owns so repair feedback
  // cannot mix a current startup failure with criteria from the prior round.
  clearPreviousGradeOutputs(args.out, declaredSuites);

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
    ...(args.observation === 'observed' ? { source: { sha256: args.sourceSha256 } } : {}),
    suites: {}, totals: {},
    selection: selection ? { ...selection, attemptedChecks: [], reportedChecks: [], notRun: [] } : null,
  };
  const selectedPackIds = new Set(selection?.checks.map(check => check.packId) ?? []);
  const selectedPackDefinitions = recipeBinding?.plan.packs
    .filter(pack => selectedPackIds.has(pack.id)) ?? [];
  const writeBundle = () => writeArtifact(join(args.out, 'bundle.json'), {
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
    const reset = resetDatabase(args);
    lastResetFailure = reset.detail;
    lastResetOutcome = reset.outcome ?? { kind: 'harness_failure', phase: 'database-reset' };
    if (!reset.ok) return false;
    // An app whose fixture data is created at startup has just had it wiped, so
    // the server has to come back before the state it seeds exists again.
    // Republishing a SpacetimeDB module re-runs `init`, so only the hosted
    // backends need this.
    const requiresReseed = executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
      'reset', 'requires-reseed');
    if (track.reseedOnReset && (args.restartSpec || args.restartCmd) && requiresReseed) {
      process.stdout.write('  reseed      ... ');
      // The restart script leaves the new server running, and on Windows that
      // child keeps a handle open long after the script's own work is done — so
      // waiting for the command to exit waits forever. What matters is whether
      // the server is answering, so that is what we wait for, and the command
      // itself is given a deadline rather than the benefit of the doubt.
      try {
        // stdio 'ignore' is what makes the timeout mean anything. The restart
        // script leaves a server running as a backgrounded descendant, and that
        // grandchild inherits the pipe: execFileSync then blocks reading a pipe
        // nobody will ever close, and the timeout does not rescue it because the
        // wait is on the pipe rather than the process. A re-baseline sat here
        // for eight hours with its server up and answering. grade.mjs already
        // does this for exactly the same command — see restartBackend.
        if (args.restartSpec) await controlBackend(args.restartSpec, 'restart');
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

      await sleep(8000);                    // let startup seeding finish
      const seeded = await verifyReseedProbe(args.reseedProbe, args.reseedProbeExpectation);
      if (!seeded.ok) {
        lastResetFailure = seeded.detail;
        lastResetOutcome = { kind: 'app_failure', phase: 'application-seed',
          appFailures: ['application-seed'] };
        console.log(`FAILED (${seeded.detail})`);
        return false;
      }
      console.log(seeded.count === null ? 'ok' : `ok (${seeded.count} entries observed)`);
      return true;
    }
    await sleep(8000);                       // let the app reconnect / republish
    return true;
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
    bundle.suites.lint = lint(args);
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
    if (r.environment?.preexistingRooms > 0) dirty = true;
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
