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
//   node run-suite.mjs --app <app-dir> --url <url> --backend spacetime|postgres|mongodb
//                      --label <id> [--out <dir>] [--media] [--level 1] [--no-reset]

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, suitesFor, DEFAULT_TRACK } from './tracks.mjs';
import { answers as hostAnswers } from './platform.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const RESET = join(ROOT, 'reset-db.sh');

function parseArgs(argv) {
  const a = { level: '1', reset: true, media: true, runIndex: 0, track: DEFAULT_TRACK };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--label': a.label = argv[++i]; break;
      case '--out': a.out = argv[++i]; break;
      case '--level': a.level = argv[++i]; break;
      case '--no-media': a.media = false; break;
      case '--track': a.track = argv[++i]; break;
      case '--reseed-probe': a.reseedProbe = argv[++i]; break;
      case '--restart-cmd': a.restartCmd = argv[++i]; break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--no-reset': a.reset = false; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.app || !a.url || !a.backend || !a.label) {
    console.error('Usage: node run-suite.mjs --app <dir> --url <url> --backend <b> --label <id> [--out <dir>] [--media] [--no-reset]');
    process.exit(2);
  }
  a.out ??= join(a.app, 'stack-bench');
  return a;
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: 'utf8', stdio: 'pipe', cwd: ROOT, ...opts });

// Does this URL respond at all? Any HTTP status counts — the question is whether
// a server is listening, not what it thinks of the request.
async function answers(url, attempts = 3) {
  for (let i = 0; i < attempts; i++) {
    if (hostAnswers(url, 4)) return true;
    await sleep(2000);
  }
  return false;
}

// The benchmark's own database containers. A generated app that connects
// somewhere else is not measuring what we think it is: one Postgres app pointed
// at an unrelated project's container on 5433 and graded "fine" while writing to
// a database the harness could not reset.
const EXPECTED_DB_PORT = { postgres: '6532', mongodb: '6537' };

function checkDatabaseProvenance(args) {
  const expected = EXPECTED_DB_PORT[args.backend];
  if (!expected) return { ok: true, reason: 'no external database for this backend' };
  const envPath = join(args.app, 'server', '.env');
  if (!existsSync(envPath)) return { ok: false, reason: 'server/.env not found' };
  const url = (readFileSync(envPath, 'utf8').match(/DATABASE_URL=(.*)/) || [])[1]?.trim() ?? '';
  const ok = url.includes(`:${expected}/`);
  return { ok, url, reason: ok ? 'ok' : `app targets ${url} but the benchmark database is on port ${expected}` };
}

// Same features for less code is a structural property of the platform, not a
// property of the model that happened to write it — unlike build cost, which
// inverted between Sonnet 4.6 and Sonnet 5.
function codeMetrics(args) {
  const SERVER_DIR = { spacetime: 'backend', postgres: 'server', mongodb: 'server' }[args.backend] ?? 'server';
  const walk = (dir, out = []) => {
    if (!existsSync(dir)) return out;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (/^(node_modules|dist|\.vite|module_bindings|drizzle)$/.test(e.name)) continue;
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p, out);
      else if (/\.(ts|tsx)$/.test(e.name)) out.push(p);
    }
    return out;
  };
  const count = files => files.reduce((n, f) => n + readFileSync(f, 'utf8').split('\n').length, 0);
  const serverFiles = walk(join(args.app, SERVER_DIR));
  const allFiles = walk(args.app);

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

function resetDatabase(args, track) {
  process.stdout.write('  reset database ... ');
  try {
    run('bash', [RESET, args.backend, args.app, String(args.runIndex ?? 0), track.slug]);
    console.log('ok');
  } catch (err) {
    console.log('FAILED');
    console.log(`    ${(err.stdout || err.message || '').toString().trim().split('\n').slice(-2).join(' | ')}`);
    return false;
  }
  return true;
}

function lint(args) {
  process.stdout.write('  contract lint ... ');
  const out = join(args.out, 'contract-lint.json');
  try {
    run('node', [join(ROOT, 'linter', 'lint.mjs'), '--url', args.url, '--level', args.level,
      '--track', args.track, '--label', args.label, '--out', out]);
  } catch { /* non-zero exit means hooks failed; the report still lands */ }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = JSON.parse(readFileSync(out, 'utf8'));
  console.log(r.pass ? `PASS (${r.counts.pass} hooks)` : `FAIL (${r.counts.fail + r.counts.blocked} missing)`);
  return r;
}

function gradeSuite(args, suite) {
  process.stdout.write(`  ${suite.id.padEnd(10)} ... `);
  const out = join(args.out, `grading-${suite.id}.json`);
  const argv = [join(ROOT, 'grader', 'grade.mjs'), '--url', args.url, '--level', args.level,
    '--label', `${args.label}-${suite.id}`, '--out', out];
  if (suite.spec) argv.push('--spec', suite.spec);
  if (args.restartCmd) argv.push('--restart-cmd', args.restartCmd);
  if (args.media) argv.push('--media', join(args.out, 'media'), '--trace');
  let stdout = '';
  try {
    stdout = run('node', argv);
  } catch (err) {
    stdout = (err.stdout || '').toString();
  }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = JSON.parse(readFileSync(out, 'utf8'));
  const dirty = r.environment?.preexistingRooms > 0 ? r.environment.preexistingRooms : 0;
  console.log(`${r.total}/${r.max}${dirty ? `  [DIRTY: ${dirty} rooms — not comparable]` : ''}`);
  for (const f of r.features) {
    for (const c of f.criteria.filter(c => !c.passed)) {
      console.log(`      FAIL ${f.name} / ${c.id}`);
    }
  }
  // A criterion that PASSED on interface behaviour alone, because its
  // server-side check could not be run against this backend, is a weaker result
  // than one where the server refused a real request. Saying so on every run is
  // the difference between a disclosed limitation and a flattering score.
  const uiOnly = r.features.flatMap(f =>
    f.criteria.filter(c => c.passed && c.serverCheck === 'unverified').map(c => `${f.name}/${c.id}`));
  if (uiOnly.length) {
    console.log(`      note: ${uiOnly.length} criterion/criteria passed on interface behaviour only`);
    for (const u of uiOnly) console.log(`            ${u} — server-side check not runnable on this backend`);
  }
  return r;
}

async function main() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  mkdirSync(args.out, { recursive: true });

  console.log(`\n=== ${args.label} (${args.backend}) ===`);
  console.log(`  app: ${args.app}`);
  console.log(`  url: ${args.url}`);

  const bundle = {
    label: args.label, track: args.track, backend: args.backend, url: args.url, app: args.app,
    level: Number(args.level), suites: {}, totals: {},
  };

  // Reset before EVERY step, not once per run: the lint and each suite create
  // state of their own, so a single up-front reset leaves later suites grading
  // dirty state — which silently lowers scores.
  const freshen = async () => {
    if (!args.reset) return true;
    if (!resetDatabase(args, track)) return false;
    // An app whose fixture data is created at startup has just had it wiped, so
    // the server has to come back before the state it seeds exists again.
    // Republishing a SpacetimeDB module re-runs `init`, so only the hosted
    // backends need this.
    if (track.reseedOnReset && args.restartCmd && args.backend !== 'spacetime') {
      process.stdout.write('  reseed      ... ');
      // The restart script leaves the new server running, and on Windows that
      // child keeps a handle open long after the script's own work is done — so
      // waiting for the command to exit waits forever. What matters is whether
      // the server is answering, so that is what we wait for, and the command
      // itself is given a deadline rather than the benefit of the doubt.
      try {
        run('bash', ['-c', args.restartCmd], { timeout: 200_000 });
        console.log('ok');
      } catch (err) {
        if (args.reseedProbe && await answers(args.reseedProbe)) {
          console.log('ok (server answering; restart command did not return)');
          await sleep(2000);
          return true;
        }
        // Say what actually went wrong. A bare "did not come back" sent the
        // first investigation looking at the application, when the fault was
        // the command line the harness built.
        const detail = ((err.stderr || '') + (err.stdout || '') + (err.message || ''))
          .toString().trim().split('\n').slice(-3).join(' | ').slice(0, 300);
        console.log('FAILED (server did not come back)');
        console.log(`    command: ${args.restartCmd}`);
        console.log(`    ${detail}`);
        return false;
      }
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
  const mutated = readdirSync(args.app, { recursive: true, withFileTypes: true })
    .filter(e => e.isFile() && e.name.endsWith('.mutation-backup'))
    .map(e => join(e.parentPath ?? e.path ?? args.app, e.name));
  if (mutated.length) {
    bundle.error = `app still carries mutation backups (${mutated.join(', ')}) — its source is mutated, not the build under test`;
    writeFileSync(join(args.out, 'bundle.json'), JSON.stringify(bundle, null, 2));
    console.log(`\nABORTED: ${bundle.error}`);
    process.exit(1);
  }

  const prov = checkDatabaseProvenance(args);
  bundle.provenance = prov;
  console.log(`  database    ... ${prov.ok ? 'benchmark-owned' : `WRONG DATABASE — ${prov.reason}`}`);
  if (!prov.ok) {
    bundle.error = `app is not using the benchmark database: ${prov.reason}`;
    writeFileSync(join(args.out, 'bundle.json'), JSON.stringify(bundle, null, 2));
    console.log('\nABORTED: results would not describe the benchmark environment.');
    process.exit(1);
  }

  if (!(await freshen())) {
    bundle.error = 'database reset failed — scores would not be comparable';
    writeFileSync(join(args.out, 'bundle.json'), JSON.stringify(bundle, null, 2));
    console.log('\nABORTED: could not reset database.');
    process.exit(1);
  }

  bundle.suites.lint = lint(args);

  let total = 0, max = 0, dirty = false;
  for (const suite of suitesFor(track, args.level)) {
    if (!(await freshen())) { console.log(`  ${suite.id}: SKIPPED (reset failed)`); continue; }
    const r = gradeSuite(args, suite);
    bundle.suites[suite.id] = r;
    if (r) {
      total += r.total; max += r.max;
      if (r.environment?.preexistingRooms > 0) dirty = true;
    }
  }

  bundle.totals = { score: total, max, dirty, contractPass: bundle.suites.lint?.pass ?? null };
  writeFileSync(join(args.out, 'bundle.json'), JSON.stringify(bundle, null, 2));

  console.log(`  ${'TOTAL'.padEnd(10)} ... ${total}/${max}${dirty ? '  [DIRTY]' : ''}`);
  console.log(`  bundle: ${join(args.out, 'bundle.json')}`);
}

main();
