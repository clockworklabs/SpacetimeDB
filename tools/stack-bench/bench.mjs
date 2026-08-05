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
// A SpacetimeDB host that is already running is used as-is and never stopped. If
// none is running one is started, and stopped again at the end unless
// --keep-spacetime.

import { execFileSync, spawn } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, cpSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, resultsName, DEFAULT_TRACK } from './tracks.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const AGENT = join(ROOT, 'agent.mjs');

const VITE_BASE = { spacetime: 6173, postgres: 6273, mongodb: 6373 };
const EXPRESS_BASE = 6001;

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
      case '--guidance': a.guidance = argv[++i]; break;
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
const SOURCE_DIRS = ['backend', 'server', 'client/src', 'client/index.html', 'client/vite.config.ts'];

function snapshotSource(appDir, to) {
  rmSync(to, { recursive: true, force: true });
  for (const rel of SOURCE_DIRS) {
    const from = join(appDir, rel);
    if (!existsSync(from)) continue;
    cpSync(from, join(to, rel), {
      recursive: true,
      filter: src => !/node_modules|[\/]dist([\/]|$)/.test(src),
    });
  }
}

function restoreSource(from, appDir) {
  for (const rel of SOURCE_DIRS) {
    const src = join(from, rel);
    if (!existsSync(src)) continue;
    rmSync(join(appDir, rel), { recursive: true, force: true });
    cpSync(src, join(appDir, rel), { recursive: true });
  }
}

// The agent starts dev servers so grading can reach them, but nothing owned
// their lifetime — runs were leaving vite and Express processes behind, which is
// how ports and app directories ended up occupied by finished work.
function portsForRun(backend, runIndex, track) {
  const offset = track.portOffset + runIndex;
  const vite = VITE_BASE[backend] + offset;
  return backend === 'spacetime' ? [vite] : [vite, EXPRESS_BASE + offset];
}

// A dev server is usually a watcher supervising a child: the child holds the
// port, the parent holds the directory. Killing by port leaves the parent alive
// and the next build fails on EBUSY, so also sweep anything whose command line
// names this run's app directory.
function stopByAppDir(appDir) {
  if (!appDir) return;
  const script = `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*${appDir.replace(/'/g, "''")}*' } | ForEach-Object { $_.ProcessId }`;
  let out = '';
  try { out = execFileSync('powershell', ['-NoProfile', '-Command', script], { encoding: 'utf8' }); }
  catch { return; }
  for (const pid of out.split(/\s+/).filter(Boolean)) {
    if (Number(pid) === process.pid) continue;
    try { execFileSync('taskkill', ['/F', '/PID', pid, '/T'], { stdio: 'ignore' }); } catch { /* already gone */ }
  }
}

function stopServers(backend, runIndex, appDir, track) {
  stopByAppDir(appDir);
  for (const port of portsForRun(backend, runIndex, track)) {
    let out = '';
    try {
      out = execFileSync('netstat', ['-ano'], { encoding: 'utf8' });
    } catch { return; }
    const pids = new Set(
      out.split(String.fromCharCode(10))
        .filter(l => new RegExp(`:${port}\\s`).test(l) && /LISTENING/i.test(l))
        .map(l => l.trim().split(/\s+/).pop())
        .filter(pid => pid && pid !== '0')
    );
    for (const pid of pids) {
      try {
        execFileSync('taskkill', ['/F', '/PID', pid, '/T'], { stdio: 'ignore' });
        console.log(`  stopped the server on :${port}`);
      } catch { /* already gone */ }
    }
  }
}

const spacetimeUp = () => {
  try {
    execFileSync('curl', ['-s', '-m', '5', '-o', process.platform === 'win32' ? 'NUL' : '/dev/null',
      'http://localhost:3000/v1/ping'], { stdio: 'ignore' });
    return true;
  } catch { return false; }
};

// Own only what you start. A host that was already up belongs to whoever started
// it — other databases live there — so it is used and left alone. One we start
// ourselves is ours to stop again, unless --keep-spacetime says otherwise.
function ensureSpacetime() {
  if (spacetimeUp()) {
    console.log('  spacetime   ... already running (will be left alone)');
    return false;
  }
  console.log('  spacetime   ... not running, starting it');
  spawn('spacetime', ['start'], { detached: true, stdio: 'ignore', shell: true }).unref();
  for (let i = 0; i < 60; i++) {
    execFileSync(process.platform === 'win32' ? 'timeout' : 'sleep',
      process.platform === 'win32' ? ['/T', '2', '/NOBREAK'] : ['2'], { stdio: 'ignore' });
    if (spacetimeUp()) { console.log('  spacetime   ... up'); return true; }
  }
  console.error('SpacetimeDB did not come up on :3000.');
  process.exit(2);
}

function stopSpacetime() {
  try {
    execFileSync('taskkill', ['/F', '/IM', 'spacetimedb-standalone.exe'], { stdio: 'ignore' });
    console.log('  stopped the SpacetimeDB host this run started');
  } catch { /* already gone */ }
}

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...opts });

function runAgent(args, mode, level, appDir) {
  const out = sh('node', [args.agent ?? AGENT, '--mode', mode, '--backend', args.backend,
    '--level', String(level), '--app', appDir, '--track', args.track,
    '--run-index', String(args.runIndex), '--model', args.model,
    '--guidance', args.guidance], { stdio: 'pipe' });
  return JSON.parse(out.trim().split('\n').pop());
}

function grade(args, appDir, url, label, level, track) {
  const expressPort = EXPRESS_BASE + track.portOffset + args.runIndex;
  const argv = [join(ROOT, 'run-suite.mjs'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--level', String(level),
    '--track', args.track,
    '--run-index', String(args.runIndex),
    ...(args.media ? [] : ['--no-media']),
    ...(args.backend === 'stub'
      ? ['--no-reset']
      : ['--restart-cmd', `bash ${join(ROOT, 'restart-backend.sh')} ${args.backend} ${appDir} ${expressPort} ${track.restartProbe}`])];
  try { sh('node', argv, { stdio: 'inherit' }); } catch { /* score is in the bundle */ }
  const bundle = join(appDir, 'stack-bench', 'bundle.json');
  return existsSync(bundle) ? JSON.parse(readFileSync(bundle, 'utf8')) : null;
}

async function main() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const url = args.url ?? `http://localhost:${VITE_BASE[args.backend] + track.portOffset + args.runIndex}`;
  const runDir = resultsName(track, args.backend, args.runIndex);
  args.out ??= join(ROOT, 'results', runDir);
  mkdirSync(args.out, { recursive: true });

  const weStartedSpacetime = args.backend === 'spacetime' ? ensureSpacetime() : false;

  // One app, grown level by level — the same app the earlier levels built.
  const appDir = args.app ?? join(ROOT, 'results', runDir, 'app');

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
    guidance: args.guidance, levels: [] };

  for (const level of args.levelList) {
    const t0 = Date.now();
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    const build = runAgent(args, level === args.levelList[0] ? 'build' : 'upgrade', level, appDir);
    // No session, no app. Grading an empty directory yields a real-looking zero
    // that is a harness failure, not a result for this backend.
    if (!build.sessionId) {
      console.log(`  ABORTED: the coding session never ran — see ${join(appDir, `.session-*-l${level}.json`)}`);
      run.levels.push({ level, score: null, max: null, error: 'coding session did not run',
        costUsd: build.costUsd, durationMs: Date.now() - t0 });
      break;
    }
    let bundle = grade(args, appDir, url, `${args.backend}-l${level}`, level, track);
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
      const snapshot = join(args.out, `snapshot-l${level}`);
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

    run.levels.push({
      level,
      score: bundle?.totals?.score ?? 0,
      max: bundle?.totals?.max ?? 0,
      contractPass: bundle?.totals?.contractPass ?? null,
      code: bundle?.code ?? null,
      buildCostUsd: build.costUsd,
      fixCostUsd: Number(fixCost.toFixed(4)),
      tokens: build.tokens,
      fixRounds,
      stalled,
      regressed,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));
  }

  run.totals = {
    // A level that never ran contributes nothing rather than NaN.
    score: run.levels.reduce((n, l) => n + (l.score ?? 0), 0),
    max: run.levels.reduce((n, l) => n + (l.max ?? 0), 0),
    costUsd: Number(run.levels.reduce((n, l) => n + (l.buildCostUsd ?? 0) + (l.fixCostUsd ?? 0), 0).toFixed(4)),
    fixRounds: run.levels.reduce((n, l) => n + (l.fixRounds ?? 0), 0),
    durationSec: Math.round((Date.now() - started) / 1000),
  };
  writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));

  console.log(`\n================ ${args.backend} summary ================`);
  for (const l of run.levels) {
    console.log(`  L${l.level}: ${l.score}/${l.max}  ${l.fixRounds} fix round(s)  ` +
      `$${(l.buildCostUsd + l.fixCostUsd).toFixed(2)}  ${l.durationSec}s`);
  }
  console.log(`  TOTAL ${run.totals.score}/${run.totals.max}  ` +
    `$${run.totals.costUsd}  ${run.totals.fixRounds} fix round(s)  ${run.totals.durationSec}s`);
  console.log(`  ${join(args.out, 'run.json')}`);

  teardown();
}

main();
