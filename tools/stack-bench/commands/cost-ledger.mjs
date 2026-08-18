#!/usr/bin/env node
// The complete token bill for one benchmark run — every session, not just the
// one that built the app.
//
// Includes nested agent transcripts and explicit harness model calls, prices
// each transcript, and reports any unreconciled residual.
//
// Usage: node commands/cost-ledger.mjs --workdir <stack-bench-runs/NAME-STAMP> [--reported <usd>]

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const argv = process.argv.slice(2);
const opt = k => { const i = argv.indexOf(k); return i === -1 ? null : argv[i + 1]; };
const workdir = opt('--workdir');
const reported = opt('--reported') ? Number(opt('--reported')) : null;
if (!workdir) { console.error('Usage: node commands/cost-ledger.mjs --workdir <run work dir> [--reported usd]'); process.exit(2); }

const PRICE = { input: 3.00, output: 15.00, cacheWrite5m: 3.75, cacheWrite1h: 6.00, cacheRead: 0.30 };
const STORE = join(homedir(), '.claude', 'projects');
const enc = p => p.replace(/[\\/:]/g, '-').toLowerCase();

function priceDir(dir) {
  const files = [];
  (function walk(d) {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.jsonl')) files.push(p);
    }
  })(dir);
  const billed = new Set();
  let usd = 0, msgs = 0;
  const efforts = new Set();
  for (const f of files) {
    for (const line of readFileSync(f, 'utf8').split('\n')) {
      if (!line.includes('"usage"')) continue;
      let rec; try { rec = JSON.parse(line); } catch { continue; }
      const u = rec?.message?.usage;
      if (!u) continue;
      const rid = rec.requestId ?? rec.uuid;
      if (billed.has(rid)) continue;
      billed.add(rid);
      msgs++;
      if (rec.effort) efforts.add(rec.effort);
      const cw = u.cache_creation ?? {};
      usd += (u.input_tokens ?? 0) * PRICE.input / 1e6
        + (u.output_tokens ?? 0) * PRICE.output / 1e6
        + (u.cache_read_input_tokens ?? 0) * PRICE.cacheRead / 1e6
        + (cw.ephemeral_5m_input_tokens ?? u.cache_creation_input_tokens ?? 0) * PRICE.cacheWrite5m / 1e6
        + (cw.ephemeral_1h_input_tokens ?? 0) * PRICE.cacheWrite1h / 1e6;
    }
  }
  return { usd, msgs, files: files.length, efforts: [...efforts] };
}

// 1. The app's own sessions (main + subagents) — filed under the app dir.
const appKey = enc(join(workdir, 'app'));
const rows = [];
for (const d of readdirSync(STORE)) {
  const dl = d.toLowerCase();
  if (dl === appKey || dl === appKey.replace(/^-+/, '')) {
    rows.push({ what: 'build + fix sessions (incl. subagents)', ...priceDir(join(STORE, d)) });
  }
}

// 2. Harness sessions that ran FOR this run, matched by time window: the
// sandbox probe and (spacetime only) the behavioural review. The work dir is
// usually swept by the time anyone audits, so the window comes from the run's
// own transcripts in the store, not from the directory on disk.
let t0 = Infinity, t1 = -Infinity;
for (const d of readdirSync(STORE)) {
  const dl = d.toLowerCase();
  if (dl !== appKey && dl !== appKey.replace(/^-+/, '')) continue;
  (function span(p) {
    for (const e of readdirSync(p, { withFileTypes: true })) {
      const q = join(p, e.name);
      if (e.isDirectory()) span(q);
      else if (e.name.endsWith('.jsonl')) {
        const m = statSync(q).mtimeMs;
        t0 = Math.min(t0, m - 90 * 60e3);   // a transcript's mtime is its END
        t1 = Math.max(t1, m + 30 * 60e3);
      }
    }
  })(join(STORE, d));
}
if (!isFinite(t0)) { console.error('no transcripts found in the store for this run'); process.exit(1); }
for (const d of readdirSync(STORE)) {
  const dl = d.toLowerCase();
  const isProbe = dl.includes('sandbox-probe');
  const isHarness = dl.endsWith('-tools-stack-bench');
  if (!isProbe && !isHarness) continue;
  const dir = join(STORE, d);
  // Only transcripts written inside this run's window.
  const inWindow = [];
  (function walk(p) {
    for (const e of readdirSync(p, { withFileTypes: true })) {
      const q = join(p, e.name);
      if (e.isDirectory()) walk(q);
      else if (e.name.endsWith('.jsonl')) {
        const m = statSync(q).mtimeMs;
        if (m >= t0 && m <= t1) inWindow.push(q);
      }
    }
  })(dir);
  if (!inWindow.length) continue;
  // Price just those files by copying the pricing loop over them.
  const billed = new Set();
  let usd = 0, msgs = 0;
  for (const f of inWindow) {
    for (const line of readFileSync(f, 'utf8').split('\n')) {
      if (!line.includes('"usage"')) continue;
      let rec; try { rec = JSON.parse(line); } catch { continue; }
      const u = rec?.message?.usage;
      if (!u) continue;
      const rid = rec.requestId ?? rec.uuid;
      if (billed.has(rid)) continue;
      billed.add(rid);
      msgs++;
      const cw = u.cache_creation ?? {};
      usd += (u.input_tokens ?? 0) * PRICE.input / 1e6
        + (u.output_tokens ?? 0) * PRICE.output / 1e6
        + (u.cache_read_input_tokens ?? 0) * PRICE.cacheRead / 1e6
        + (cw.ephemeral_5m_input_tokens ?? u.cache_creation_input_tokens ?? 0) * PRICE.cacheWrite5m / 1e6
        + (cw.ephemeral_1h_input_tokens ?? 0) * PRICE.cacheWrite1h / 1e6;
    }
  }
  rows.push({ what: isProbe ? 'sandbox probe (harness)' : 'behavioural review / harness sessions', usd, msgs, files: inWindow.length, efforts: [] });
}

console.log(`\nledger for ${workdir}\n`);
let total = 0;
for (const r of rows) {
  total += r.usd;
  console.log(`  ${r.what.padEnd(42)} ${String(r.msgs).padStart(4)} msgs  ${('$' + r.usd.toFixed(2)).padStart(8)}${r.efforts.length ? `  effort=${r.efforts.join(',')}` : ''}`);
}
console.log(`  ${'TOTAL ACCOUNTED'.padEnd(42)} ${''.padStart(9)} ${('$' + total.toFixed(2)).padStart(8)}`);
if (reported != null) {
  const buildRows = rows.filter(r => r.what.startsWith('build'));
  const buildUsd = buildRows.reduce((n, r) => n + r.usd, 0);
  const diff = reported - buildUsd;
  console.log(`\n  run.json reported (build+fix only)          ${('$' + reported.toFixed(2)).padStart(8)}`);
  console.log(`  reconstruction of the same sessions         ${('$' + buildUsd.toFixed(2)).padStart(8)}`);
  console.log(`  RESIDUAL (reported minus reconstructed)     ${('$' + diff.toFixed(2)).padStart(8)}  ${(100 * Math.abs(diff) / reported).toFixed(1)}%`);
  console.log('  A nonzero residual means the CLI billed something these transcripts do');
  console.log('  not carry. It is bounded and disclosed, not explained.');
}
console.log('');
