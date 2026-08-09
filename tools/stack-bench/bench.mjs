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
//                  [--keep-spacetime] [--no-media]
//
// The benchmark runs its own SpacetimeDB host (STACK_BENCH_STDB_URI, default
// 127.0.0.1:3210, data in .spacetime-data) rather than a machine-wide one, so
// resource measurements describe the module under test and a durability restart
// cannot disturb anything else. It is started if absent and stopped at the end
// unless --keep-spacetime.

import { execFileSync, spawn } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, cpSync, rmSync, renameSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { loadTrack, resultsName, portsFor, workDirFor, sweepWorkRoot, assertNoPortCollisions, PORT_BASES, DEFAULT_TRACK } from './tracks.mjs';
import { pidsOnPort, pidsMatching, killTree, sleepSync, answers } from './platform.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const AGENT = join(ROOT, 'agent.mjs');

function parseArgs(argv) {
  const a = { model: 'claude-sonnet-5', fixRounds: 3, runIndex: 0, levels: '1', media: true,
    guidance: 'prescribed', track: DEFAULT_TRACK };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': a.backend = argv[++i]; break;
      case '--track': a.track = argv[++i]; break;
      case '--levels': a.levels = argv[++i]; break;
      case '--model': a.model = argv[++i]; break;
      case '--fix-rounds': a.fixRounds = parseInt(argv[++i], 10); break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--out': a.out = argv[++i]; break;
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--agent': a.agent = argv[++i]; break;
      case '--no-media': a.media = false; break;
      case '--keep-spacetime': a.keepSpacetime = true; break;
      case '--stack': a.guidance = argv[++i] === 'free' ? 'minimal' : 'prescribed'; break;
      case '--guidance': a.guidance = argv[++i]; break;
      case '--skip-probe': a.skipProbe = true; break;
      // Which reference documents to inline (spacetime only). The variable
      // under test in the cost work; passed straight through to agent.mjs.
      case '--skills': a.skills = argv[++i]; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.backend) {
    console.error('Usage: node bench.mjs --backend <b> --levels 1-3 [--fix-rounds 3] [--run-index N]');
    process.exit(2);
  }
  const [from, to] = a.levels.split('-').map(Number);
  a.levelList = Array.from({ length: (to ?? from) - from + 1 }, (_, i) => from + i);
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
    for (let attempt = 0; ; attempt++) {
      try { rmSync(dest, { recursive: true, force: true }); break; }
      catch (err) {
        if (attempt >= 5) throw err;
        sleepSync(2000);
      }
    }
    cpSync(src, dest, { recursive: true });
  }
}

// The agent starts dev servers so grading can reach them, but nothing owned
// their lifetime — runs were leaving vite and Express processes behind, which is
// how ports and app directories ended up occupied by finished work.
function portsForRun(backend, runIndex, track) {
  // The stub backend (offline test loop) owns no ports at all.
  if (!PORT_BASES[backend]) return [];
  const p = portsFor(track, backend, runIndex);
  return p.express ? [p.vite, p.express] : [p.vite];
}

// A dev server is usually a watcher supervising a child: the child holds the
// port, the parent holds the directory. Killing by port leaves the parent alive
// and the next build fails on EBUSY, so also sweep anything whose command line
// names this run's app directory.
function stopByAppDir(appDir) {
  if (!appDir) return;
  for (const pid of pidsMatching(appDir)) killTree(pid);
}

function stopServers(backend, runIndex, appDir, track) {
  stopByAppDir(appDir);
  for (const port of portsForRun(backend, runIndex, track)) {
    for (const pid of pidsOnPort(port)) {
      killTree(pid);
      console.log(`  stopped the server on :${port}`);
    }
  }
}

// The benchmark runs its OWN SpacetimeDB host, on its own port and data
// directory. Sharing a machine-wide host meant resource measurements described
// whatever else was published there, and restarting it for a durability test
// took somebody else's databases down with it.
const STDB_URI = process.env.STACK_BENCH_STDB_URI ?? 'http://127.0.0.1:3210';
const STDB_PORT = new URL(STDB_URI).port;
const STDB_DATA_DIR = join(ROOT, '.spacetime-data');
// The host under test is the one built from this repository. Falling back to a
// PATH install would benchmark a release nobody is changing.
const LOCAL_CLI = join(ROOT, '..', '..', 'target', 'release', 'spacetimedb-cli.exe');
const SPACETIME_BIN = process.env.SPACETIME_BIN ?? (existsSync(LOCAL_CLI) ? LOCAL_CLI : 'spacetime');

const spacetimeUp = () => answers(`${STDB_URI}/v1/ping`, 5);

// Own only what you start. A host that was already up belongs to whoever started
// it — other databases live there — so it is used and left alone. One we start
// ourselves is ours to stop again, unless --keep-spacetime says otherwise.
function ensureSpacetime() {
  if (spacetimeUp()) {
    console.log('  spacetime   ... already running (will be left alone)');
    return false;
  }
  console.log(`  spacetime   ... not running, starting a benchmark-owned host on :${STDB_PORT}`);
  mkdirSync(STDB_DATA_DIR, { recursive: true });
  spawn(SPACETIME_BIN, ['start', '--listen-addr', `127.0.0.1:${STDB_PORT}`, '--data-dir', STDB_DATA_DIR],
    { detached: true, stdio: 'ignore', shell: true }).unref();
  for (let i = 0; i < 60; i++) {
    sleepSync(2000);
    if (spacetimeUp()) { console.log('  spacetime   ... up'); return true; }
  }
  console.error(`SpacetimeDB did not come up on :${STDB_PORT}.`);
  process.exit(2);
}

// Stop ONLY the host listening on the benchmark's port. Killing by image name
// terminates every SpacetimeDB on the machine, which is how a grading run once
// took down databases that had nothing to do with the benchmark.
function stopSpacetime() {
  for (const pid of pidsOnPort(STDB_PORT)) {
    killTree(pid);
    console.log(`  stopped the SpacetimeDB host this run started on :${STDB_PORT}`);
  }
}

// Which SpacetimeDB produced this result. With a local build that moves under
// us, "the version" stops being inferable from the date, and an unrecorded
// variable has already cost this project several reversed conclusions.
function stdbProvenance() {
  const out = { cli: null, commit: null, sdk: null, skillRevision: null };
  try {
    const v = execFileSync(SPACETIME_BIN, ['--version'], { encoding: 'utf8' });
    out.cli = (v.match(/tool version ([\d.]+)/) || [])[1] ?? null;
    out.commit = (v.match(/Commit: ([0-9a-f]+)/) || [])[1] ?? null;
  } catch { /* not built, or PATH fallback */ }
  const repo = join(ROOT, '..', '..');
  try {
    out.sdk = JSON.parse(readFileSync(join(repo, 'crates', 'bindings-typescript', 'package.json'), 'utf8')).version;
  } catch { /* not a checkout */ }
  try {
    out.skillRevision = execFileSync('git', ['-C', repo, 'hash-object',
      'skills/typescript-server/SKILL.md'], { encoding: 'utf8' }).trim();
  } catch { /* not a git checkout */ }
  return out;
}

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...opts });

function runAgent(args, mode, level, appDir) {
  const out = sh('node', [args.agent ?? AGENT, '--mode', mode, '--backend', args.backend,
    '--level', String(level), '--app', appDir, '--track', args.track,
    '--run-index', String(args.runIndex), '--model', args.model,
    '--guidance', args.guidance,
    ...(args.skills ? ['--skills', args.skills] : [])], { stdio: 'pipe' });
  return JSON.parse(out.trim().split('\n').pop());
}

// The restart command is handed to `bash -c`, which reads a backslash as an
// escape: a Windows path arrives with its separators eaten and `\tools` turned
// into a literal tab. Bash accepts forward slashes on Windows, so the path is
// written the one way both shells agree on.
const forBash = p => p.replace(/\\/g, '/');

function grade(args, appDir, url, label, level, track) {
  const expressPort = PORT_BASES[args.backend]
    ? portsFor(track, args.backend, args.runIndex).express ?? ''
    : '';
  const argv = [join(ROOT, 'run-suite.mjs'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--level', String(level),
    '--track', args.track,
    '--reseed-probe', `http://localhost:${expressPort}${track.restartProbe}`,
    '--run-index', String(args.runIndex),
    ...(args.media ? [] : ['--no-media']),
    ...(args.backend === 'stub'
      ? ['--no-reset']
      : ['--restart-cmd', `bash ${forBash(join(ROOT, 'restart-backend.sh'))} ${args.backend} ${forBash(appDir)} ${expressPort} ${track.restartProbe}`])];
  try { sh('node', argv, { stdio: 'inherit' }); } catch { /* score is in the bundle */ }
  const bundle = join(appDir, 'stack-bench', 'bundle.json');
  return existsSync(bundle) ? JSON.parse(readFileSync(bundle, 'utf8')) : null;
}

async function main() {
  const args = parseArgs(process.argv);
  process.env.STACK_BENCH_STDB_URI = STDB_URI;
  process.env.SPACETIME_BIN = SPACETIME_BIN;
  process.env.STACK_BENCH_STDB_OWNED = '1';   // this host is ours; restarts are allowed
  const track = loadTrack(args.track);
  assertNoPortCollisions();

  // Prove the sandbox before spending a run on it. The rules have already been
  // wrong twice in ways that read as fine: a deny list shipped under
  // --dangerously-skip-permissions enforced nothing at all, and the first probe
  // to check it reported a pass by matching the word "denied" inside the file it
  // had just read. Neither showed up as an error — both would have produced a
  // full set of confident, void scores. One cheap session up front is worth less
  // than the run it protects.
  // The stub backend is the offline test loop: no model, no cost, nothing to
  // protect. Spending a real CLI session probing it would make the one test
  // that is supposed to run for free stop being free.
  if (!args.skipProbe && args.backend !== 'stub') {
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
  args.out ??= join(ROOT, 'results', runDir);
  mkdirSync(args.out, { recursive: true });

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

  const weStartedSpacetime = args.backend === 'spacetime' ? ensureSpacetime() : false;

  // One app, grown level by level — the same app the earlier levels built.
  // Built OUTSIDE the harness. While the app lived at results/<run>/app it sat
  // underneath the thing grading it: two directories up are the scenario files
  // and grade.mjs, and transcripts show builds taking exactly that walk. An
  // isolated root removes the class rather than forbidding instances of it.
  // Artifacts are copied back to results/ when the run finishes.
  // Finished runs are deleted on the way in rather than left to pile up in temp,
  // and anything still locked is named instead of silently skipped.
  const stuck = sweepWorkRoot();
  if (stuck.length) {
    console.log(`  workdirs   ... ${stuck.length} old run dir(s) could not be removed (still held):`);
    for (const d of stuck.slice(0, 3)) console.log(`               ${d}`);
    console.log('               harmless — this run uses its own directory.');
  }

  // Stamped, so this run cannot inherit a directory another one is still
  // holding. Every level shares it: L2 upgrades the app L1 built.
  //
  // An EXPLICIT --app belongs to the caller — the test loop passes one, and
  // deleting its parent on the way out deleted the loop's own run.json. Only a
  // directory this run created is this run's to remove.
  const ownWorkDir = !args.app;
  const stamp = new Date().toISOString().replace(/[-:T]/g, '').slice(0, 14);
  const appDir = args.app ?? join(workDirFor(track, args.backend, args.runIndex, stamp), 'app');

  // Leave nothing running once the run is over, however it ends — but only stop
  // what this run brought up.
  const teardown = () => {
    stopServers(args.backend, args.runIndex, appDir, track);
    if (weStartedSpacetime && !args.keepSpacetime) stopSpacetime();
  };
  // A previous run may have left a watcher holding the app directory, which the
  // build's wipe would trip over.
  stopServers(args.backend, args.runIndex, appDir, track);
  process.on('SIGINT', () => { console.log('interrupted — stopping servers'); teardown(); process.exit(130); });
  process.on('SIGTERM', () => { teardown(); process.exit(143); });

  const started = Date.now();
  const run = { track: args.track, backend: args.backend, model: args.model,
    guidance: args.guidance, stack: args.guidance === 'minimal' ? 'free' : 'prescribed', levels: [] };

  for (const level of args.levelList) {
    const t0 = Date.now();
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    const build = runAgent(args, level === args.levelList[0] ? 'build' : 'upgrade', level, appDir);
    // Carry the agent's own record of the setup up to the run. Comparing two
    // scores is only meaningful if the reasoning budget, permission mode and
    // CLI version behind them were the same, and that is not knowable after the
    // fact unless it was written down at the time.
    run.setup ??= build.setup;
    // No session, no app. Grading an empty directory yields a real-looking zero
    // that is a harness failure, not a result for this backend.
    if (!build.sessionId) {
      console.log(`  ABORTED: the coding session never ran — see ${join(appDir, `.session-*-l${level}.json`)}`);
      run.levels.push({ level, score: null, max: null, error: 'coding session did not run',
        costUsd: build.costUsd, durationMs: Date.now() - t0 });
      break;
    }
    let bundle = grade(args, appDir, url, `${args.backend}-l${level}`, level, track);

    // What the model built BEFORE being handed the answers. Every backend can
    // reach the same total given enough fix rounds, so the post-fix score stops
    // discriminating — what it got right unaided is the comparison that survives.
    const firstBuild = {
      score: bundle?.totals?.score ?? null,
      max: bundle?.totals?.max ?? null,
      contractPass: bundle?.totals?.contractPass ?? null,
      missed: Object.values(bundle?.suites ?? {}).flatMap(s =>
        (s?.features ?? []).flatMap(f =>
          (f.criteria ?? []).filter(c => !c.passed).map(c => `${f.name}/${c.id}`))),
    };

    let fixRounds = 0;
    let fixCost = 0;
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
      // A fix can break more than it mends. Keep the source that produced the
      // best score so far, and roll back to it if a round regresses.
      // Kept outside the results tree: a snapshot is a known-good copy of the
      // answer, and a coding session that can reach one will copy it instead of
      // building. It only has to survive this process.
      const snapshot = join(tmpdir(), `stack-bench-snapshot-${args.backend}-${args.track}-run${args.runIndex}-l${level}`);
      snapshotSource(appDir, snapshot);
      fixRounds += 1;
      console.log(`--- fix round ${fixRounds}/${args.fixRounds} ---`);
      const fix = runAgent(args, 'fix', level, appDir);
      fixCost += fix.costUsd;
      bundle = grade(args, appDir, url, `${args.backend}-l${level}-fix${fixRounds}`, level, track);

      // A round that moves nothing usually means the finding is not actionable —
      // often the harness is wrong, not the app. Stop rather than pay again for
      // the same result.
      const after = bundle?.totals?.score ?? 0;
      if (after < before) {
        console.log(`    regressed (${before} -> ${after}); rolling back and stopping`);
        // Stop the servers BEFORE deleting what they are watching. Without
        // this, rolling back a regressed postgres run threw EBUSY on
        // app/server and took the whole finished run down with it.
        stopServers(args.backend, args.runIndex, appDir, track);
        restoreSource(snapshot, appDir);
        bundle = grade(args, appDir, url, `${args.backend}-l${level}-rollback`, level, track);
        regressed = true;
        stalled = true;
        break;
      }
      if (after === before) {
        console.log(`    no improvement (${before}); stopping fix rounds`);
        stalled = true;
        break;
      }
    }

    // A grading run that crashed writes no bundle, and recording that as 0/0
    // makes a harness failure indistinguishable from an app that scored nothing
    // — in a ladder run it silently drops a level's result on the floor. Say so
    // instead, and leave the score null.
    const graded = bundle?.totals?.max > 0;
    if (!graded) {
      console.log(`  L${level}: GRADING DID NOT COMPLETE — no usable bundle. ` +
        `Score is unknown, not zero; re-grade this level before using the run.`);
    }
    run.levels.push({
      level,
      graded,
      score: graded ? bundle.totals.score : null,
      max: graded ? bundle.totals.max : null,
      firstBuild,
      contractPass: bundle?.totals?.contractPass ?? null,
      code: bundle?.code ?? null,
      buildCostUsd: build.costUsd,
      fixCostUsd: Number(fixCost.toFixed(4)),
      tokens: build.tokens,
      // Carried up so a run summary can explain a cost, not just report one.
      usage: build.usage ?? null,
      turns: build.turns ?? null,
      promptBytes: build.promptBytes ?? null,
      tokensPerTurn: build.tokensPerTurn ?? null,
      fixRounds,
      stalled,
      regressed,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));
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
    sh('node', [join(ROOT, 'archive-transcripts.mjs'), '--app', appDir, '--label', runDir],
      { stdio: 'pipe' });
  } catch { console.log('  (transcript archiving failed — evidence is on a 30-day timer)'); }

  run.totals = {
    // A level that never ran contributes nothing rather than NaN.
    score: run.levels.reduce((n, l) => n + (l.score ?? 0), 0),
    max: run.levels.reduce((n, l) => n + (l.max ?? 0), 0),
    costUsd: Number(run.levels.reduce((n, l) => n + (l.buildCostUsd ?? 0) + (l.fixCostUsd ?? 0), 0).toFixed(4)),
    fixRounds: run.levels.reduce((n, l) => n + (l.fixRounds ?? 0), 0),
    durationSec: Math.round((Date.now() - started) / 1000),
    // Which levels the totals are actually made of. A run missing a level is
    // not comparable with one that graded them all, and the summary has to
    // carry that rather than leaving it to be noticed.
    ungraded: run.levels.filter(l => !l.graded).map(l => l.level),
  };
  writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));

  // What the model fought with is the part SpacetimeDB can act on, and it is
  // only in the transcript — the score cannot say it. Appended to a running
  // file after every SpacetimeDB run so the pattern across runs is visible
  // rather than rediscovered each time.
  if (args.backend === 'spacetime') {
    try {
      sh('node', [join(ROOT, 'stdb-report.mjs'), '--label', runDir, '--track', args.track,
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
      sh('node', [join(ROOT, 'stdb-review.mjs'), '--label', runDir,
        '--source', join(args.out, 'source'),
        '--compare', ['postgres', 'mongodb']
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
}

main();
