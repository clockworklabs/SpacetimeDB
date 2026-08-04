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
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') a.app = argv[++i];
    else if (argv[i] === '--url') a.url = argv[++i];
    else if (argv[i] === '--mutations') a.mutations = argv[++i];
    else if (argv[i] === '--level') a.level = argv[++i];
    else { console.error(`Unknown arg ${argv[i]}`); process.exit(2); }
  }
  if (!a.app || !a.url || !a.mutations) {
    console.error('Usage: node mutation-test.mjs --app <dir> --url <url> --mutations <file> [--level N]');
    process.exit(2);
  }
  a.level ??= '1';
  return a;
}

const sleep = ms => new Promise(r => setTimeout(r, ms));

function grade(url, level, feature) {
  const args = ['grade.mjs', '--url', url, '--level', level, '--out', '.mutation-report.json'];
  if (feature) args.push('--feature', String(feature));
  try {
    execFileSync('node', args, { stdio: 'pipe', encoding: 'utf8' });
  } catch { /* grader exits non-zero only on infra errors; report is what matters */ }
  const r = JSON.parse(readFileSync('.mutation-report.json', 'utf8'));
  return r;
}

const args = parseArgs(process.argv);
const spec = JSON.parse(readFileSync(args.mutations, 'utf8'));

console.log('Baseline (unmutated app)...');
const baseline = grade(args.url, args.level);
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
  await sleep(m.settleMs ?? 4000);            // let the dev server hot-reload

  let r;
  try {
    r = grade(args.url, args.level);
  } finally {
    copyFileSync(backup, target);
    unlinkSync(backup);
    await sleep(m.settleMs ?? 4000);          // restore before the next mutation
  }

  const got = Object.fromEntries(r.features.map(f => [f.id, f.score]));
  const expectedDropped = got[m.breaks] < baseScores[m.breaks];
  const collateral = Object.keys(baseScores)
    .filter(id => Number(id) !== m.breaks && got[id] < baseScores[id])
    .map(Number);

  const status = !expectedDropped ? 'SURVIVED' : collateral.length ? 'CAUGHT+COLLATERAL' : 'CAUGHT';
  results.push({ id: m.id, status, breaks: m.breaks, before: baseScores[m.breaks], after: got[m.breaks], collateral });

  const detail = `F${m.breaks} ${baseScores[m.breaks]}→${got[m.breaks]}` +
    (collateral.length ? `, also dropped F${collateral.join(',F')}` : '');
  console.log(`${status.padEnd(18)} ${m.id} — ${detail}`);
  if (status === 'SURVIVED') console.log(`    ORACLE HOLE: ${m.desc}`);
}

if (existsSync('.mutation-report.json')) unlinkSync('.mutation-report.json');

const survived = results.filter(r => r.status === 'SURVIVED');
console.log(`\n${results.filter(r => r.status.startsWith('CAUGHT')).length}/${results.length} mutations caught`);
if (survived.length) {
  console.log('Oracle holes — the grader does not detect these defects:');
  for (const s of survived) console.log(`  - ${s.id}`);
  process.exit(1);
}
