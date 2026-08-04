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
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(ROOT, '..', '..');
const RESET = join(REPO, 'tools', 'llm-sequential-upgrade', 'reset-app.sh');

const SUITES = [
  { id: 'features', spec: null, key: 'level' },
  { id: 'invariants', spec: 'scenarios/level-01-invariants.json' },
  { id: 'delivery', spec: 'scenarios/level-01-delivery.json' },
];

function parseArgs(argv) {
  const a = { level: '1', reset: true, media: false };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--label': a.label = argv[++i]; break;
      case '--out': a.out = argv[++i]; break;
      case '--level': a.level = argv[++i]; break;
      case '--media': a.media = true; break;
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
const run = (cmd, args) => execFileSync(cmd, args, { encoding: 'utf8', stdio: 'pipe', cwd: ROOT });

// The benchmark's own database containers. A generated app that connects
// somewhere else is not measuring what we think it is: one Postgres app pointed
// at an unrelated project's container on 5433 and graded "fine" while writing to
// a database the harness could not reset.
const EXPECTED_DB_PORT = { postgres: '6432', mongodb: '6437' };

function checkDatabaseProvenance(args) {
  const expected = EXPECTED_DB_PORT[args.backend];
  if (!expected) return { ok: true, reason: 'spacetime module — no external database' };
  const envPath = join(args.app, 'server', '.env');
  if (!existsSync(envPath)) return { ok: false, reason: 'server/.env not found' };
  const url = (readFileSync(envPath, 'utf8').match(/DATABASE_URL=(.*)/) || [])[1]?.trim() ?? '';
  const ok = url.includes(`:${expected}/`);
  return { ok, url, reason: ok ? 'ok' : `app targets ${url} but the benchmark database is on port ${expected}` };
}

function resetDatabase(args) {
  process.stdout.write('  reset database ... ');
  try {
    run('bash', [RESET, args.app]);
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
      '--label', args.label, '--out', out]);
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
  if (suite.spec) argv.push('--spec', join(ROOT, suite.spec));
  if (args.media) argv.push('--media', join(args.out, 'media'));
  let stdout = '';
  try {
    stdout = run('node', argv);
  } catch (err) {
    stdout = (err.stdout || '').toString();
  }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = JSON.parse(readFileSync(out, 'utf8'));
  const dirty = r.environment?.preexistingRooms;
  console.log(`${r.total}/${r.max}${dirty ? `  [DIRTY: ${dirty} rooms — not comparable]` : ''}`);
  for (const f of r.features) {
    for (const c of f.criteria.filter(c => !c.passed)) {
      console.log(`      FAIL ${f.name} / ${c.id}`);
    }
  }
  return r;
}

async function main() {
  const args = parseArgs(process.argv);
  mkdirSync(args.out, { recursive: true });

  console.log(`\n=== ${args.label} (${args.backend}) ===`);
  console.log(`  app: ${args.app}`);
  console.log(`  url: ${args.url}`);

  const bundle = {
    label: args.label, backend: args.backend, url: args.url, app: args.app,
    level: Number(args.level), suites: {}, totals: {},
  };

  // Reset before EVERY step, not once per run: the lint and each suite create
  // rooms of their own, so a single up-front reset leaves later suites grading
  // dirty state — which silently lowers scores.
  const freshen = async () => {
    if (!args.reset) return true;
    if (!resetDatabase(args)) return false;
    await sleep(8000);                       // let the app reconnect / republish
    return true;
  };

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
  for (const suite of SUITES) {
    if (!(await freshen())) { console.log(`  ${suite.id}: SKIPPED (reset failed)`); continue; }
    const r = gradeSuite(args, suite);
    bundle.suites[suite.id] = r;
    if (r) {
      total += r.total; max += r.max;
      if (r.environment?.preexistingRooms) dirty = true;
    }
  }

  bundle.totals = { score: total, max, dirty, contractPass: bundle.suites.lint?.pass ?? null };
  writeFileSync(join(args.out, 'bundle.json'), JSON.stringify(bundle, null, 2));

  console.log(`  ${'TOTAL'.padEnd(10)} ... ${total}/${max}${dirty ? '  [DIRTY]' : ''}`);
  console.log(`  bundle: ${join(args.out, 'bundle.json')}`);
}

main();
