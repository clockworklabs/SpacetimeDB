#!/usr/bin/env node
// Account for a run's cost from its transcript, and check the accounting.
//
// Every cost claim in this project so far has been a decomposition I did by
// hand, with prices I assumed, and the last one left $0.87 of a $4.00 gap
// unexplained. This reads the per-message usage the CLI actually recorded, adds
// it up exactly, and reconciles the result against the total the CLI reported.
// If those two do not agree, the model is wrong and the output says so rather
// than presenting a tidy breakdown that happens to be fiction.
//
// Usage:
//   node cost-audit.mjs <transcripts-dir> [--since 2026-08-09T16:30] [--expect 11.28]

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const args = process.argv.slice(2);
const dir = args.find(a => !a.startsWith('--'));
const opt = k => { const i = args.indexOf(k); return i === -1 ? null : args[i + 1]; };
if (!dir) { console.error('Usage: node cost-audit.mjs <transcripts-dir> [--since ISO] [--expect USD]'); process.exit(2); }
const since = opt('--since') ? new Date(opt('--since')).getTime() : 0;
const expect = opt('--expect') ? Number(opt('--expect')) : null;

// Published Sonnet rates, $ per million tokens. Stated here rather than buried
// so a wrong assumption is visible and correctable — the reconciliation below
// is what tells you whether they are right.
const PRICE = {
  input: 3.00,
  output: 15.00,
  cacheWrite5m: 3.75,
  cacheWrite1h: 6.00,
  cacheRead: 0.30,
};

// Recurse. Subagent transcripts live at <project>/<sessionId>/subagents/, and
// missing them left $1.44 of an $11.28 run unaccounted — a session that spawns
// help is not cheaper for it, it is just billed somewhere else on disk.
function walk(d, out = []) {
  for (const e of readdirSync(d, { withFileTypes: true })) {
    const p = join(d, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith('.jsonl') && statSync(p).mtimeMs >= since) out.push(p);
  }
  return out;
}
const files = walk(dir).sort((a, b) => statSync(a).mtimeMs - statSync(b).mtimeMs);

const turns = [];
// One API request can appear several times in a transcript — a resumed or
// re-serialised assistant message carries the same requestId and the same usage
// block. Counting them all doubled a reconstruction: 290 records for 154 real
// requests, and $14.30 against a reported $7.27. Bill each requestId once.
const billed = new Set();
const efforts = new Map();
const models = new Map();
const tiers = new Map();

for (const f of files) {
  for (const line of readFileSync(f, 'utf8').split('\n')) {
    if (!line.includes('"usage"')) continue;
    let rec; try { rec = JSON.parse(line); } catch { continue; }
    const u = rec?.message?.usage;
    if (!u) continue;
    const rid = rec.requestId ?? rec.uuid;
    if (rid) { if (billed.has(rid)) continue; billed.add(rid); }
    if (rec.effort) efforts.set(rec.effort, (efforts.get(rec.effort) ?? 0) + 1);
    if (rec.message?.model) models.set(rec.message.model, (models.get(rec.message.model) ?? 0) + 1);
    if (u.service_tier) tiers.set(u.service_tier, (tiers.get(u.service_tier) ?? 0) + 1);
    const cw = u.cache_creation ?? {};
    turns.push({
      ts: Date.parse(rec.timestamp ?? '') || 0,
      input: u.input_tokens ?? 0,
      output: u.output_tokens ?? 0,
      read: u.cache_read_input_tokens ?? 0,
      write5m: cw.ephemeral_5m_input_tokens ?? (u.cache_creation_input_tokens ?? 0),
      write1h: cw.ephemeral_1h_input_tokens ?? 0,
    });
  }
}

if (!turns.length) { console.error(`no usage records in ${dir} (after --since)`); process.exit(1); }

const sum = k => turns.reduce((n, t) => n + t[k], 0);
const tot = {
  input: sum('input'), output: sum('output'), read: sum('read'),
  write5m: sum('write5m'), write1h: sum('write1h'),
};
const cost = {
  input: tot.input * PRICE.input / 1e6,
  output: tot.output * PRICE.output / 1e6,
  cacheRead: tot.read * PRICE.cacheRead / 1e6,
  cacheWrite5m: tot.write5m * PRICE.cacheWrite5m / 1e6,
  cacheWrite1h: tot.write1h * PRICE.cacheWrite1h / 1e6,
};
const total = Object.values(cost).reduce((a, b) => a + b, 0);

console.log(`\n${dir}`);
console.log(`  ${files.length} transcript(s), ${turns.length} billed messages`);
console.log(`  model  : ${[...models.keys()].join(', ') || 'unknown'}`);
console.log(`  effort : ${[...efforts].map(([e, n]) => `${e} (${n})`).join(', ') || 'not recorded'}`);
console.log(`  tier   : ${[...tiers].map(([t, n]) => `${t} (${n})`).join(', ') || 'unknown'}`);

console.log('\n  component        tokens        $      share');
const rows = [
  ['cache read', tot.read, cost.cacheRead],
  ['cache write 5m', tot.write5m, cost.cacheWrite5m],
  ['cache write 1h', tot.write1h, cost.cacheWrite1h],
  ['output', tot.output, cost.output],
  ['input (uncached)', tot.input, cost.input],
];
for (const [name, tk, $] of rows) {
  console.log(`  ${name.padEnd(16)} ${(tk / 1e6).toFixed(2).padStart(7)}M  ${('$' + $.toFixed(2)).padStart(7)}  ${(100 * $ / total).toFixed(1).padStart(5)}%`);
}
console.log(`  ${'TOTAL'.padEnd(16)} ${((tot.read + tot.write5m + tot.write1h + tot.output + tot.input) / 1e6).toFixed(2).padStart(7)}M  ${('$' + total.toFixed(2)).padStart(7)}`);

if (expect != null) {
  const diff = total - expect;
  const pct = 100 * Math.abs(diff) / expect;
  console.log(`\n  reconciliation vs the CLI's reported total`);
  console.log(`    reconstructed $${total.toFixed(2)}   reported $${expect.toFixed(2)}   difference $${diff.toFixed(2)} (${pct.toFixed(1)}%)`);
  console.log(pct <= 5
    ? '    within 5% — the price model and the usage records agree, so a breakdown from them can be trusted.'
    : '    MORE THAN 5% APART — do not trust a breakdown built on these prices until this is explained.');
}

// Where the money accrues as the run proceeds. Cache reads grow with the
// conversation, so this shows whether cost is front-loaded or compounding.
const q = Math.ceil(turns.length / 4);
console.log('\n  spend by quarter of the run (cache read tokens, $):');
for (let i = 0; i < 4; i++) {
  const slice = turns.slice(i * q, (i + 1) * q);
  if (!slice.length) continue;
  const r = slice.reduce((n, t) => n + t.read, 0);
  const c = slice.reduce((n, t) =>
    n + (t.read * PRICE.cacheRead + t.output * PRICE.output + t.write5m * PRICE.cacheWrite5m
      + t.write1h * PRICE.cacheWrite1h + t.input * PRICE.input) / 1e6, 0);
  console.log(`    Q${i + 1}  ${slice.length.toString().padStart(3)} msgs  ${(r / 1e6).toFixed(2)}M read  $${c.toFixed(2)}`);
}
console.log('');
