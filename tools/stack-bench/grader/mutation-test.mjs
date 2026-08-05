#!/usr/bin/env node
// Stack Bench grader validation by mutation testing.
//
// A grader that never fails anything is worthless, and one that fails the wrong
// thing is worse. This deliberately breaks a KNOWN-GOOD app one defect at a
// time and checks that the grader (a) notices, and (b) notices in the right
// feature and nowhere else.
//
// A mutation that survives — score unchanged — is a hole in the oracle.
//
// Usage: node mutation-test.mjs --app <app-dir> --url <url> --mutations mutations/<file>.json
//
// The mutation file lists client-source edits (find/replace) plus which feature
// each is expected to break. Client edits hot-reload, so no redeploy is needed.

import { readFileSync, writeFileSync, copyFileSync, existsSync, unlinkSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

// Resolve tooling relative to this file so the runner works from any directory.
const HERE = dirname(fileURLToPath(import.meta.url));
const GRADER = join(HERE, 'grade.mjs');
const REPORT = join(HERE, '.mutation-report.json');

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') a.app = argv[++i];
    else if (argv[i] === '--url') a.url = argv[++i];
    else if (argv[i] === '--mutations') a.mutations = argv[++i];
    else if (argv[i] === '--level') a.level = argv[++i];
    else if (argv[i] === '--spec') a.spec = argv[++i];
    else if (argv[i] === '--backend') a.backend = argv[++i];
    else if (argv[i] === '--run-index') a.runIndex = argv[++i];
    else if (argv[i] === '--track-slug') a.slug = argv[++i];
    else if (argv[i] === '--probe') a.probe = argv[++i];
    else if (argv[i] === '--redeploy') a.redeploy = argv[++i];
    else { console.error(`Unknown arg ${argv[i]}`); process.exit(2); }
  }
  if (!a.app || !a.url || !a.mutations) {
    console.error('Usage: node mutation-test.mjs --app <dir> --url <url> --mutations <file> [--level N] [--backend <b>]');
    process.exit(2);
  }
  a.level ??= '1';
  a.runIndex ??= '0';
  return a;
}

const sleep = ms => new Promise(r => setTimeout(r, ms));
const lastLine = err => (err.stdout || err.message || '').toString().trim().split(/\r?\n/).pop();

// Editing a watched source file restarts the server. Grading before it is back
// fails EVERY feature, which reads as "the mutation was caught" in the target
// and as collateral everywhere else — three probes were wasted that way. Wait
// for the app to answer instead of guessing with a sleep.
async function waitForApp(a, seconds = 120) {
  const probe = new URL(a.probe ?? '/api/rooms', a.url).toString();
  const deadline = Date.now() + seconds * 1000;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(probe, { signal: AbortSignal.timeout(3000) });
      if (res.status < 500) return true;      // 401/404 still means it is serving
    } catch { /* not up yet */ }
    await sleep(2000);
  }
  console.log(`  (app did not answer at ${probe} within ${seconds}s — results will be unreliable)`);
  return false;
}

// Grading a dirty database silently lowers scores, and this compares scores
// across runs — an accumulated room would read as a mutation being "caught".
function reset(a) {
  if (!a.backend) return;
  try {
    execFileSync('bash', [join(HERE, '..', 'reset-db.sh'), a.backend, a.app, a.runIndex, a.slug ?? ''],
      { stdio: 'pipe', encoding: 'utf8' });
  } catch (err) {
    console.log(`  (reset failed: ${(err.stdout || err.message || '').toString().trim().split('\n').pop()})`);
  }
}

function redeploy(a, spec) {
  const cmd = a.redeploy ?? spec.redeploy;
  if (!cmd) return;
  try { execFileSync('bash', ['-c', cmd], { cwd: a.app, stdio: 'pipe', encoding: 'utf8' }); }
  catch (err) { console.log(`  (redeploy failed: ${lastLine(err)})`); }
}

function grade(a, feature) {
  reset(a);
  const args = [GRADER, '--url', a.url, '--level', a.level, '--out', REPORT];
  if (a.spec) args.push('--spec', a.spec);
  if (feature) args.push('--feature', String(feature));
  try {
    execFileSync('node', args, { stdio: 'pipe', encoding: 'utf8' });
  } catch { /* grader exits non-zero only on infra errors; report is what matters */ }
  const r = JSON.parse(readFileSync(REPORT, 'utf8'));
  return r;
}

const args = parseArgs(process.argv);
const spec = JSON.parse(readFileSync(args.mutations, 'utf8'));

console.log('Baseline (unmutated app)...');
await waitForApp(args);
const baseline = grade(args, undefined);
const baseScores = Object.fromEntries(baseline.features.map(f => [f.id, f.score]));
console.log(`  baseline: ${baseline.total}/${baseline.max}  ${baseline.features.map(f => `F${f.id}:${f.score}`).join(' ')}\n`);

const results = [];
for (const m of spec.mutations) {
  const target = join(args.app, m.file);
  const backup = `${target}.mutation-backup`;
  const original = readFileSync(target, 'utf8');
  const edits = m.edits ?? [{ find: m.find, replace: m.replace }];

  const missing = edits.find(e => !original.includes(e.find));
  if (missing) {
    console.log(`SKIP  ${m.id} — anchor not found in ${m.file} (app changed?)`);
    results.push({ id: m.id, status: 'SKIP', reason: 'anchor not found' });
    continue;
  }

  copyFileSync(target, backup);
  writeFileSync(target, edits.reduce((src, e) => src.replace(e.find, e.replace), original));
  await sleep(m.settleMs ?? 4000);            // let the watcher notice the edit
  redeploy(args, spec);
  await waitForApp(args);

  let r;
  try {
    r = grade(args, undefined);
  } finally {
    copyFileSync(backup, target);
    unlinkSync(backup);
    await sleep(m.settleMs ?? 4000);
    redeploy(args, spec);
    await waitForApp(args);                   // healthy again before the next probe
  }

  const got = Object.fromEntries(r.features.map(f => [f.id, f.score]));
  const expectedDropped = got[m.breaks] < baseScores[m.breaks];
  const collateral = Object.keys(baseScores)
    .filter(id => Number(id) !== m.breaks && got[id] < baseScores[id])
    .map(Number);

  const status = !expectedDropped ? 'SURVIVED' : collateral.length ? 'CAUGHT+COLLATERAL' : 'CAUGHT';

  // WHICH criterion noticed matters as much as whether the score moved. A
  // feature dropping to zero usually means its setup broke, which proves the
  // app is wrecked rather than that the intended assertion can see the defect.
  const broken = r.features.find(f => f.id === m.breaks);
  const caughtBy = (broken?.criteria ?? []).filter(c => !c.passed).map(c => c.id);
  const setupBroke = Boolean(broken?.setupError);
  results.push({ id: m.id, status, breaks: m.breaks, before: baseScores[m.breaks], after: got[m.breaks],
    collateral, caughtBy, setupBroke, setupError: broken?.setupError ?? null });

  const detail = `F${m.breaks} ${baseScores[m.breaks]}→${got[m.breaks]}` +
    (collateral.length ? `, also dropped F${collateral.join(',F')}` : '');
  console.log(`${status.padEnd(18)} ${m.id} — ${detail}`);
  if (caughtBy.length) console.log(`    failed criteria: ${caughtBy.join(', ')}`);
  if (setupBroke) {
    console.log(`    WARNING: the feature's SETUP failed (${String(broken.setupError).slice(0, 120)})`);
    console.log('    The score moved because the app broke, not because the assertion saw the defect.');
    console.log('    Treat this as an inconclusive probe and make the mutation more surgical.');
  }
  if (status === 'SURVIVED') console.log(`    ORACLE HOLE: ${m.desc}`);
}

if (existsSync(REPORT)) unlinkSync(REPORT);

const survived = results.filter(r => r.status === 'SURVIVED');
console.log(`\n${results.filter(r => r.status.startsWith('CAUGHT')).length}/${results.length} mutations caught`);
if (survived.length) {
  console.log('Oracle holes — the grader does not detect these defects:');
  for (const s of survived) console.log(`  - ${s.id}`);
  process.exit(1);
}
