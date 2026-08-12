#!/usr/bin/env node
// Read a directory of load.mjs reports and say what, if anything, they support.
//
// The first sweep produced a headline that did not survive more data, because a
// single cell was read as a result. This reports the SPREAD across repeats and
// refuses to call a difference real when the repeats overlap: if backend A's
// range and backend B's range intersect, the honest statement is "no separation
// at this sample size", not whichever midpoint happens to be lower.
//
// Usage: node summarise.mjs <dir>

import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { readArtifactPayload } from '../artifacts.mjs';

const dir = process.argv[2];
if (!dir) { console.error('Usage: node summarise.mjs <dir>'); process.exit(2); }

const rows = readdirSync(dir).filter(f => f.endsWith('.json'))
  .map(f => readArtifactPayload(join(dir, f)));

// cells[backend][config] = [report, ...]
const cells = {};
for (const r of rows) {
  const m = /-([A-Z])-r\d+$/.exec(r.label) ?? /-([A-Z])$/.exec(r.label);
  const config = m ? m[1] : '?';
  ((cells[r.backend] ??= {})[config] ??= []).push(r);
}

const CONFIG_MEANING = {
  A: 'baseline            ',
  B: '3x fanout, same data',
  C: 'same fanout, 3x data',
};

const range = xs => ({ lo: Math.min(...xs), hi: Math.max(...xs), mid: xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)] });
const fmt = r => `${r.mid}ms [${r.lo}-${r.hi}]`;

console.log('\nDelivery latency p50, median of repeats with full range\n');
const backends = Object.keys(cells).sort();
for (const cfg of ['A', 'B', 'C']) {
  console.log(`  ${cfg}  ${CONFIG_MEANING[cfg] ?? ''}`);
  for (const b of backends) {
    const reps = cells[b]?.[cfg] ?? [];
    if (!reps.length) continue;
    const p50 = range(reps.map(r => r.deliveryLatencyMs.p50));
    const p95 = range(reps.map(r => r.deliveryLatencyMs.p95));
    const lost = reps.reduce((n, r) => n + r.lost, 0);
    const samples = reps.reduce((n, r) => n + r.delivered, 0);
    console.log(`      ${b.padEnd(10)} p50 ${fmt(p50).padEnd(22)} p95 ${fmt(p95).padEnd(24)} ${samples} samples, ${lost} lost`);
  }
}

// Overlapping ranges mean the repeats do not separate the backends, whatever
// the midpoints say.
console.log('\nSeparation check (p50 ranges across repeats)\n');
for (const cfg of ['A', 'B', 'C']) {
  const present = backends.filter(b => cells[b]?.[cfg]?.length);
  for (let i = 0; i < present.length; i++) {
    for (let j = i + 1; j < present.length; j++) {
      const xs = cells[present[i]][cfg], ys = cells[present[j]][cfg];
      const x = range(xs.map(r => r.deliveryLatencyMs.p50));
      const y = range(ys.map(r => r.deliveryLatencyMs.p50));
      const overlap = x.lo <= y.hi && y.lo <= x.hi;
      // A single run per cell has no spread, so every comparison would read as
      // separated — which is precisely the false confidence that produced a
      // retracted headline. One repeat supports no verdict at all.
      const verdict = Math.min(xs.length, ys.length) < 2
        ? `INSUFFICIENT REPEATS (${xs.length} vs ${ys.length}) — no verdict`
        : overlap
          ? 'OVERLAP — no separation at this sample size'
          : `SEPARATED — ${x.hi < y.lo ? present[i] : present[j]} is faster`;
      console.log(`  ${cfg}  ${present[i]} vs ${present[j]}: ${verdict}`);
    }
  }
}

// Fanout and volume effects, per backend, as ratios against that backend's own
// baseline — the comparison that survives backends having different baselines.
console.log('\nEffect of each factor, relative to that backend\'s own baseline\n');
for (const b of backends) {
  const base = cells[b]?.A?.length ? range(cells[b].A.map(r => r.deliveryLatencyMs.p50)).mid : null;
  if (!base) continue;
  const line = [];
  for (const cfg of ['B', 'C']) {
    if (!cells[b]?.[cfg]?.length) continue;
    const mid = range(cells[b][cfg].map(r => r.deliveryLatencyMs.p50)).mid;
    line.push(`${cfg} ${mid > base ? '+' : ''}${Math.round((mid - base) / base * 100)}%`);
  }
  console.log(`  ${b.padEnd(10)} baseline ${base}ms   ${line.join('   ')}`);
}

console.log('\nServer cost (CPU-seconds per 1000 delivered, median of repeats)\n');
for (const b of backends) {
  const all = Object.values(cells[b]).flat().map(r => r.cpuSecondsPer1kDelivered).filter(n => n != null);
  const rss = Object.values(cells[b]).flat().map(r => r.server.peakRssBytes).filter(Boolean);
  if (!all.length) continue;
  console.log(`  ${b.padEnd(10)} ${fmt(range(all)).replace(/ms/g, '')}  peak RSS ${Math.round(range(rss).hi / 1e6)}MB`);
}
console.log('');
