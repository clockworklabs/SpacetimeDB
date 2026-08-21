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
//   node commands/compare-runs.mjs results/postgres-ecom-run0 results/spacetime-ecom-run0

import { existsSync, readdirSync } from 'node:fs';
import { join, basename } from 'node:path';
import { readArtifactPayload } from '../src/evidence/artifacts.mjs';
import { criterionEvidence, evidenceDisposition } from '../src/evidence/check-evidence.mjs';
import { evidenceStatusLabel } from '../src/evidence/evidence-presentation.mjs';

const dirs = process.argv.slice(2).filter(a => !a.startsWith('--'));
if (dirs.length < 2) {
  console.error('Usage: node commands/compare-runs.mjs <run-dir> <run-dir> [...]');
  process.exit(2);
}

// One run -> { name, cost, criteria: Map<id, {points, passed, inconclusive}> }
function loadRun(dir) {
  const gdir = join(dir, 'grading');
  if (!existsSync(gdir)) throw new Error(`no grading/ in ${dir} — was this run graded?`);
  const criteria = new Map();
  const recipeHashes = new Set();
  const selectionHashes = new Set();
  for (const f of readdirSync(gdir)) {
    if (!f.startsWith('grading-') || !f.endsWith('.json')) continue;
    const suite = f.slice('grading-'.length, -'.json'.length);
    const r = readArtifactPayload(join(gdir, f));
    const recipeHash = r.artifactEnvelope?.identities?.recipe?.sha256;
    if (recipeHash) recipeHashes.add(recipeHash);
    if (r.selection?.sha256) selectionHashes.add(r.selection.sha256);
    for (const feat of r.features ?? []) {
      for (const c of feat.criteria ?? []) {
        const evidence = criterionEvidence(c);
        const disposition = evidenceDisposition(evidence);
        const detail = String(evidence.summary ?? '');
        criteria.set(c.stableKey ?? `${suite}/${c.id}`, {
          points: c.points ?? 1,
          passed: disposition.passed,
          measured: disposition.measured,
          status: evidence.status,
          detail,
        });
      }
    }
  }
  let cost = null;
  const rj = join(dir, 'run.json');
  if (existsSync(rj)) cost = readArtifactPayload(rj)?.totals?.costUsd ?? null;
  if (recipeHashes.size > 1) throw new Error(`${dir} contains more than one recipe identity`);
  if (selectionHashes.size > 1) throw new Error(`${dir} contains more than one selection identity`);
  return { name: basename(dir), cost, criteria,
    recipeSha256: [...recipeHashes][0] ?? null,
    selectionSha256: [...selectionHashes][0] ?? null };
}

const runs = dirs.map(loadRun);
const unidentified = runs.filter(run => !run.recipeSha256 || !run.selectionSha256);
if (unidentified.length) {
  throw new Error(`cannot prove comparable run scope for ${unidentified.map(run => run.name).join(', ')}; `
    + 'use artifacts that record recipe and selection identities');
}
const identified = runs.filter(run => run.recipeSha256 && run.selectionSha256);
const scopeKeys = new Set(identified.map(run => `${run.recipeSha256}:${run.selectionSha256}`));
if (scopeKeys.size > 1) {
  throw new Error('runs use different recipe or selection identities and are not directly comparable');
}
// Every criterion anyone attempted.
const all = new Set(runs.flatMap(r => [...r.criteria.keys()]));

const common = [];
const notEverywhere = [];
for (const id of [...all].sort()) {
  const seen = runs.map(r => r.criteria.get(id));
  const measuredEverywhere = seen.every(c => c?.measured);
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
      if (!c.measured) return `${r.name}: ${evidenceStatusLabel(c.status)}`;
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
