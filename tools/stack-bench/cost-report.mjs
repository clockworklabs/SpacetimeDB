#!/usr/bin/env node
// What a build actually cost, and why.
//
// A single dollar figure invites the wrong conclusion. Cost here is roughly
// turns × context size: cache reads dominate the token count and are re-paid on
// every turn, so a backend handed a larger guidance pack pays more for the same
// work. Two of the three inputs — how much documentation we hand each
// contestant, and how many turns its toolchain forces — belong to the harness,
// not the database. This report separates them so a cost gap can be attributed.
//
// Usage: node cost-report.mjs [--track ecommerce] [--run-index 0]

import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, listTracks, resultsName, DEFAULT_TRACK } from './tracks.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const arg = (name, fallback) => {
  const i = process.argv.indexOf(name);
  return i === -1 ? fallback : process.argv[i + 1];
};
const trackName = arg('--track', DEFAULT_TRACK);
const runIndex = Number(arg('--run-index', '0'));
if (!listTracks().includes(trackName)) {
  console.error(`Unknown track "${trackName}". Available: ${listTracks().join(', ')}`);
  process.exit(2);
}
const track = loadTrack(trackName);

const num = n => (n == null ? '—' : n.toLocaleString('en-US'));
const rows = [];

for (const backend of ['spacetime', 'postgres', 'mongodb']) {
  const dir = join(ROOT, 'results', resultsName(track, backend, runIndex));
  const runFile = join(dir, 'run.json');
  if (!existsSync(runFile)) continue;
  const level = JSON.parse(readFileSync(runFile, 'utf8')).levels?.[0];
  if (!level) continue;

  // Older runs predate the detailed capture; fall back to the session record.
  let extra = {};
  const session = join(dir, 'app', '.session-build-l1.json');
  if (existsSync(session)) {
    try { extra = JSON.parse(readFileSync(session, 'utf8')); } catch { /* ignore */ }
  }
  const usage = level.usage ?? extra.usage ?? null;
  const bundle = join(dir, 'app', 'stack-bench', 'bundle.json');
  const code = existsSync(bundle) ? JSON.parse(readFileSync(bundle, 'utf8')).code : null;

  rows.push({
    backend,
    costUsd: level.buildCostUsd,
    minutes: Math.round(level.durationSec / 60),
    turns: level.turns ?? extra.turns ?? null,
    promptBytes: level.promptBytes ?? extra.promptBytes ?? null,
    tokens: level.tokens,
    output: usage?.output ?? extra.outputTokens ?? null,
    cacheRead: usage?.cacheRead ?? null,
    perTurn: level.tokensPerTurn ?? extra.tokensPerTurn ?? null,
    serverLoc: code?.serverLoc ?? null,
    deps: code?.runtimeDeps ?? null,
  });
}

if (!rows.length) {
  console.error(`No runs found for track "${trackName}" at --run-index ${runIndex}.`);
  process.exit(2);
}

console.log(`\nBuild economics — ${trackName}, run ${runIndex}`);
console.log('(costUsd is the CLI\'s API-equivalent figure; a subscription run spends plan usage, not money)\n');

const head = ['backend', 'cost', 'min', 'turns', 'prompt B', 'total tok', 'output', 'cache read', 'tok/turn', 'LOC', 'deps'];
const widths = [10, 8, 4, 6, 9, 12, 8, 11, 9, 6, 5];
const line = cells => cells.map((c, i) => String(c).padStart(widths[i])).join('  ');
console.log(line(head));
console.log(widths.map(w => '─'.repeat(w)).join('  '));
for (const r of rows) {
  console.log(line([r.backend, '$' + (r.costUsd ?? 0).toFixed(2), r.minutes, num(r.turns),
    num(r.promptBytes), num(r.tokens), num(r.output), num(r.cacheRead), num(r.perTurn),
    num(r.serverLoc), num(r.deps)]));
}

// The comparison that matters: output tokens measure work done, everything else
// measures what the harness made them carry to do it.
const out = rows.filter(r => r.output != null);
if (out.length > 1) {
  const spread = Math.max(...out.map(r => r.output)) / Math.min(...out.map(r => r.output));
  const tot = Math.max(...rows.map(r => r.tokens)) / Math.min(...rows.map(r => r.tokens));
  console.log(`\n  output tokens vary ${spread.toFixed(2)}× across backends — how much code each actually wrote.`);
  console.log(`  total tokens vary ${tot.toFixed(2)}× — the gap is context re-read per turn, not work done.`);
}
const withPrompt = rows.filter(r => r.promptBytes);
if (withPrompt.length > 1) {
  const big = withPrompt.reduce((a, b) => (a.promptBytes > b.promptBytes ? a : b));
  const small = withPrompt.reduce((a, b) => (a.promptBytes < b.promptBytes ? a : b));
  console.log(`  guidance asymmetry: ${big.backend} is handed ${(big.promptBytes / small.promptBytes).toFixed(1)}× ${small.backend}'s prompt` +
    ` — level these before quoting any cost comparison.`);
}
console.log();
