#!/usr/bin/env node
// Compare runs on the SAME criteria, and say plainly which ones could not be
// measured everywhere.
//
// Why this exists: a criterion the grader could not evaluate is subtracted from
// that run's denominator (grade.mjs, `result.max -= criterion.points`). Per run
// that is the honest thing to do — an unmeasurable check should not count
// against an app. Across runs it is a trap. SpacetimeDB scored 48/48 and
// PostgreSQL 50/50 on the same level, which reads as a tie and is not one: two
// contention criteria were never measured on SpacetimeDB, because the harness
// replays captured HTTP writes and that client writes over WebSocket.
//
// So the comparison here is on the INTERSECTION: every criterion that produced
// a real verdict everywhere. Anything else is reported separately rather than
// averaged away, because "we could not test this on one stack" is a finding
// about the harness, not a score.
//
// Usage:
//   node compare-runs.mjs results/postgres-ecom-run0 results/spacetime-ecom-run0

import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { join, basename } from 'node:path';

const dirs = process.argv.slice(2).filter(a => !a.startsWith('--'));
if (dirs.length < 2) {
  console.error('Usage: node compare-runs.mjs <run-dir> <run-dir> [...]');
  process.exit(2);
}

// One run -> { name, cost, criteria: Map<id, {points, passed, inconclusive}> }
function loadRun(dir) {
  const gdir = join(dir, 'grading');
  if (!existsSync(gdir)) throw new Error(`no grading/ in ${dir} — was this run graded?`);
  const criteria = new Map();
  for (const f of readdirSync(gdir)) {
    if (!f.startsWith('grading-') || !f.endsWith('.json')) continue;
    const suite = f.slice('grading-'.length, -'.json'.length);
    const r = JSON.parse(readFileSync(join(gdir, f), 'utf8'));
    for (const feat of r.features ?? []) {
      for (const c of feat.criteria ?? []) {
        const detail = String(c.detail ?? '');
        criteria.set(`${suite}/${c.id}`, {
          points: c.points ?? 1,
          passed: c.passed === true,
          // The grader marks these by prefixing the detail; mirror that rather
          // than inventing a second convention.
          inconclusive: c.passed !== true && /^INCONCLUSIVE/.test(detail),
          detail,
        });
      }
    }
  }
  let cost = null;
  const rj = join(dir, 'run.json');
  if (existsSync(rj)) cost = JSON.parse(readFileSync(rj, 'utf8'))?.totals?.costUsd ?? null;
  return { name: basename(dir), cost, criteria };
}

const runs = dirs.map(loadRun);

// Every criterion anyone attempted.
const all = new Set(runs.flatMap(r => [...r.criteria.keys()]));

const common = [];
const notEverywhere = [];
for (const id of [...all].sort()) {
  const seen = runs.map(r => r.criteria.get(id));
  const measuredEverywhere = seen.every(c => c && !c.inconclusive);
  (measuredEverywhere ? common : notEverywhere).push(id);
}

const w = Math.max(...runs.map(r => r.name.length), 8);
console.log(`\nComparable criteria: ${common.length} of ${all.size}\n`);
console.log(`  ${'run'.padEnd(w)}  ${'score'.padStart(9)}  ${'cost'.padStart(8)}`);
for (const r of runs) {
  let score = 0, max = 0;
  for (const id of common) {
    const c = r.criteria.get(id);
    max += c.points;
    if (c.passed) score += c.points;
  }
  const cost = r.cost == null ? '—' : `$${r.cost.toFixed(2)}`;
  console.log(`  ${r.name.padEnd(w)}  ${`${score}/${max}`.padStart(9)}  ${cost.padStart(8)}`);
}

if (notEverywhere.length) {
  console.log(`\nNOT comparable — no verdict on at least one run (${notEverywhere.length}):`);
  for (const id of notEverywhere) {
    const where = runs.map(r => {
      const c = r.criteria.get(id);
      if (!c) return `${r.name}: absent`;
      if (c.inconclusive) return `${r.name}: INCONCLUSIVE`;
      return `${r.name}: ${c.passed ? 'pass' : 'fail'}`;
    }).join('  |  ');
    console.log(`  ${id}\n      ${where}`);
  }
  console.log('\nThese are excluded from the scores above. A criterion that cannot be');
  console.log('measured on one stack is a gap in the harness, not a point for either side.');
}

// Where the runs actually disagree — the only rows worth arguing about.
const differing = common.filter(id => new Set(runs.map(r => r.criteria.get(id).passed)).size > 1);
console.log(`\nDisagreements on comparable criteria: ${differing.length}`);
for (const id of differing) {
  console.log(`  ${id}`);
  for (const r of runs) {
    const c = r.criteria.get(id);
    console.log(`      ${r.name.padEnd(w)} ${c.passed ? 'pass' : 'FAIL'}${c.passed ? '' : `  ${c.detail.slice(0, 90)}`}`);
  }
}
console.log('');
