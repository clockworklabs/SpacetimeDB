#!/usr/bin/env node
// What did SpacetimeDB cost the model, and where did it get stuck?
//
// The score says which backend finished. It does not say what the model fought
// with on the way, and that is the part SpacetimeDB can act on: an error it hit
// seven times is a documentation or API problem with a name and a fix, where
// "cost more" is only a complaint.
//
// Token figures come from the CLI's own `usage` on each assistant message --
// real input/output/cache counts, not bytes divided by four. An estimate is
// fine for a rough share; it is not fine for a number somebody is going to
// prioritise engineering work against.
//
// Reads the ARCHIVED transcripts under transcripts/<label>/ rather than the
// CLI's live store, because the live copies are pruned after 30 days and this
// file is meant to accumulate across runs.
//
// Usage:
//   node dist/commands/stdb-report.js --label spacetime-ecom-run0 [--track ecommerce]
//                        [--level 1] [--score 47/49] [--cost 7.04]
//                        [--out local-notes/STDB-FRICTION.md] [--print]

import { readFileSync, readdirSync, existsSync, appendFileSync, writeFileSync, statSync, openSync as fsOpenSync, closeSync, rmSync, mkdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';

import { operationalOutputRoot } from '../src/runtime/operational-paths.js';

import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';
const OPERATIONAL_ROOT = operationalOutputRoot(ROOT);

// Concurrent runs append here: n=5 isolated trials all write one friction
// record, and interleaved appends can split an entry down the middle. An
// exclusive-create lock serialises them; a stale lock older than a minute is
// broken rather than deadlocking a benchmark run over a log file.
function appendLocked(file: string, text: string): void {
  const lock = file + '.lock';
  for (let i = 0; i < 120; i++) {
    try {
      const fd = fsOpenSync(lock, 'wx');
      try { appendFileSync(file, text); } finally { closeSync(fd); rmSync(lock, { force: true }); }
      return;
    } catch (error: unknown) {
      if (!(error instanceof Error && 'code' in error && error.code === 'EEXIST')) throw error;
      try {
        if (Date.now() - statSync(lock).mtimeMs > 60000) rmSync(lock, { force: true });
      } catch { /* someone else cleared it */ }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
    }
  }
  appendFileSync(file, text);   // lock never freed: a garbled entry beats a lost one
}

function argument(name: string): string | undefined;
function argument(name: string, defaultValue: string): string;
function argument(name: string, defaultValue?: string): string | undefined {
  const index = process.argv.indexOf(name);
  return index === -1 ? defaultValue : process.argv[index + 1] ?? defaultValue;
}

const label = argument('--label');
if (!label) { console.error('need --label <transcripts folder>'); process.exit(2); }
const OUT = resolve(OPERATIONAL_ROOT, argument('--out', 'local-notes/STDB-FRICTION.md'));
mkdirSync(dirname(OUT), { recursive: true });
const dir = join(OPERATIONAL_ROOT, 'transcripts', label);
if (!existsSync(dir)) { console.error(`no archived transcripts at ${dir}`); process.exit(2); }

// ── Reading the transcripts ────────────────────────────────────────────────

const files = readdirSync(dir).filter(f => f.endsWith('.jsonl'))
  .map(f => ({ f, m: statSync(join(dir, f)).mtimeMs }))
  .sort((a, b) => a.m - b.m);

// Only this run's sessions. The archive folder is keyed by run label, so
// earlier runs of the same backend pile up in it; taking everything would
// blend a fixed problem back into today's numbers.
const sinceMs = Number(argument('--since-ms', '0')) || ((files.at(-1)?.m ?? 0) - 6 * 3600_000);
const mine = files.filter(f => f.m >= sinceMs);

const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, turns: 0 };
const reads = new Map<string, number>();
interface Failure { tool: string; cmd: string; body: string }
const failures: Failure[] = [];
let toolCalls = 0, blocked = 0;

for (const { f } of mine) {
  const pending = new Map<string, Omit<Failure, 'body'>>();
  for (const line of readFileSync(join(dir, f), 'utf8').split('\n')) {
    if (!line.trim()) continue;
    let event: Record<string, unknown>;
    try { event = JSON.parse(line) as Record<string, unknown>; } catch { continue; }

    const message = event.message as Record<string, unknown> | undefined;
    const u = message?.usage as Record<string, number> | undefined;
    if (u !== undefined) {
      usage.turns++;
      usage.input += u.input_tokens ?? 0;
      usage.output += u.output_tokens ?? 0;
      usage.cacheRead += u.cache_read_input_tokens ?? 0;
      usage.cacheWrite += u.cache_creation_input_tokens ?? 0;
    }

    const content = message?.content;
    if (!Array.isArray(content)) continue;
    for (const part of content as Array<Record<string, unknown>>) {
      const p = part;
      if (p.type === 'tool_use') {
        toolCalls++;
        const input = p.input as Record<string, unknown> | undefined;
        const id = String(p.id ?? '');
        const tool = String(p.name ?? '');
        pending.set(id, { tool, cmd: String(input?.command ?? input?.file_path ?? '') });
        const path = input?.file_path;
        if (path && /^(Read|NotebookRead)$/.test(tool)) {
          const k = String(path).replace(/\\/g, '/');
          reads.set(k, (reads.get(k) ?? 0) + 1);
        }
      } else if (p.type === 'tool_result') {
        const toolUseId = String(p.tool_use_id ?? '');
        const meta = pending.get(toolUseId);
        if (meta === undefined) continue;
        pending.delete(toolUseId);
        const body = typeof p.content === 'string' ? p.content : JSON.stringify(p.content ?? '');
        // A sandbox refusal is the harness working, not the model struggling.
        // Counting it as build friction would blame SpacetimeDB for our own
        // deny rules.
        if (/requested permissions|permission settings|denied by|requires approval|was blocked|blocked by/i
          .test(body)) { blocked++; continue; }
        // Grader output narrates its own failures; those are scores, not the
        // build fighting the SDK.
        if (/^SCENARIO |golden path aborted/m.test(body)) continue;
        // A Bash command that "succeeded" can still be a compiler or the CLI
        // saying no, so the text is searched as well as the error flag.
        //
        // The SpacetimeDB phrasings are listed explicitly because leaving them
        // out lost the most valuable signal there is: a first pass reported two
        // trivial failures and hid `Pre-publish check failed ... invalid bsatn
        // module def` and the `--delete-data` migration abort, which are the
        // exact things this report exists to surface.
        const failed = p.is_error === true
          || /\berror TS\d+|compilation failed|Build failed|not assignable|Cannot find (?:name|module)/i.test(body)
          || /Pre-publish check failed|Aborting because|^\s*error:\s/mi.test(body);
        if (failed) failures.push({ ...meta, body });
      }
    }
  }
}

// ── Turning failures into something actionable ─────────────────────────────

// Collapse an error to its shape so the same problem hit five times counts as
// one problem with a frequency, which is the form a fix gets prioritised in.
function signature(body: string): string {
  const ts = body.match(/error TS(\d+):\s*([^\n]{0,120})/);
  if (ts) {
    return `TS${ts[1]}: ${ts[2]!
      .replace(/'[^']*'/g, "'…'")            // the specific symbol varies
      .replace(/\s+/g, ' ').trim()}`;
  }
  // The SpacetimeDB CLI's own refusals, matched on how it actually phrases them.
  // A looser "any line containing the word error" caught SOURCE CODE instead --
  // `? err.message : String(err))` and a stray `events. */` were both reported
  // as SpacetimeDB errors, which would have sent someone hunting a bug that was
  // a comment.
  const cli = body.match(/(Pre-publish check failed[^\n]{0,140}|Aborting because[^\n]{0,140}|^\s*error:\s*[^\n]{0,120})/m);
  if (cli) {
    return `spacetimedb: ${cli[1]!.replace(/[0-9a-f]{8,}/g, '…').replace(/\s+/g, ' ').trim()}`;
  }
  const npm = body.match(/npm (?:ERR!|error)[^\n]{0,100}/);
  if (npm) return npm[0].replace(/\s+/g, ' ').trim();
  const first = body.split('\n').find(line => /error|failed|cannot/i.test(line));
  return (first ?? body.slice(0, 100)).replace(/[0-9a-f]{8,}/g, '…').replace(/\s+/g, ' ').trim().slice(0, 120);
}

interface ErrorCluster { sig: string; count: number; tool: string }
const clusters = new Map<string, ErrorCluster>();
for (const f of failures) {
  const sig = signature(f.body);
  const c = clusters.get(sig) ?? { sig, count: 0, tool: f.tool };
  c.count++;
  clusters.set(sig, c);
}
const topErrors = [...clusters.values()].sort((a, b) => b.count - a.count).slice(0, 8);

// Which SpacetimeDB surface was involved. A count against a surface is what
// tells the team whether the pain is the schema API, the client SDK, or the CLI.
const SURFACES: ReadonlyArray<readonly [RegExp, string]> = [
  [/module_bindings|generate/i, 'generated bindings'],
  [/reducer|ctx\.db|schema|table|index/i, 'server API (schema / reducers)'],
  [/subscribe|subscription|onInsert|onUpdate|connection|identity/i, 'client SDK (subscriptions)'],
  [/publish|spacetime(db)?[- ]cli|--delete-data/i, 'CLI / publish'],
  [/view|filter|query/i, 'views / queries'],
];
const surfaceOf = (text: string): string => SURFACES.find(([pattern]) => pattern.test(text))?.[1] ?? 'other';
const bySurface = new Map<string, number>();
for (const f of failures) {
  const s = surfaceOf(`${f.cmd}\n${f.body}`);
  bySurface.set(s, (bySurface.get(s) ?? 0) + 1);
}

const topReads = [...reads.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
const bindingsReads = [...reads.entries()]
  .filter(([p]) => /module_bindings/.test(p))
  .reduce((n, [, c]) => n + c, 0);

// ── The running file ───────────────────────────────────────────────────────

const formattedNumber = (value: number): string => value.toLocaleString();
const percent = (value: number, total: number): string => total
  ? `${Math.round((value / total) * 100)}%` : '—';
const totalIn = usage.input + usage.cacheRead + usage.cacheWrite;

const stamp = argument('--date') ?? new Date().toISOString().slice(0, 16).replace('T', ' ');
const track = argument('--track');
const level = argument('--level');
const lines: string[] = [];
lines.push(`## ${stamp} — ${label}${track ? ` (${track})` : ''}${level ? ` L${level}` : ''}`);
lines.push('');
if (process.argv.includes('--contaminated')) {
  lines.push('> ⚠️ **This run was contaminated** — the build read the harness that grades it.');
  lines.push('> The friction below still happened, but it is a floor rather than a measurement:');
  lines.push('> a build with the answers fights the SDK less than one without them.');
  lines.push('');
}
const score = argument('--score');
const cost = argument('--cost');
const fixRounds = argument('--fix-rounds');
if (score || cost) {
  lines.push(`**Result:** ${score ?? '—'}${cost ? `, $${cost}` : ''}`
    + `${fixRounds ? `, ${fixRounds} fix round(s)` : ''}`);
  lines.push('');
}
lines.push(`**Tokens** (from the CLI's own usage, ${mine.length} session(s), ${usage.turns} turns)`);
lines.push('');
lines.push('| | tokens | share of input |');
lines.push('|---|---:|---:|');
lines.push(`| cache read | ${formattedNumber(usage.cacheRead)} | ${percent(usage.cacheRead, totalIn)} |`);
lines.push(`| cache write | ${formattedNumber(usage.cacheWrite)} | ${percent(usage.cacheWrite, totalIn)} |`);
lines.push(`| fresh input | ${formattedNumber(usage.input)} | ${percent(usage.input, totalIn)} |`);
lines.push(`| output | ${formattedNumber(usage.output)} | — |`);
lines.push('');

if (topErrors.length) {
  lines.push(`**Where it got stuck** — ${failures.length} build failure(s) of ${toolCalls} tool calls`
    + `${blocked ? ` (plus ${blocked} refused by the sandbox — harness, not SpacetimeDB)` : ''}`);
  lines.push('');
  lines.push('| times | error |');
  lines.push('|---:|---|');
  for (const e of topErrors) lines.push(`| ${e.count} | ${e.sig.replace(/\|/g, '\\|')} |`);
  lines.push('');
  if (bySurface.size) {
    lines.push('By SpacetimeDB surface: '
      + [...bySurface].sort((a, b) => b[1] - a[1]).map(([s, c]) => `${s} (${c})`).join(', '));
    lines.push('');
  }
} else {
  lines.push('**Where it got stuck** — nothing failed. Worth checking the run actually did the work.');
  lines.push('');
}

lines.push(`**Re-read** — ${bindingsReads} read(s) of generated bindings`);
lines.push('');
for (const [p, c] of topReads) lines.push(`- ${c}x \`${p.split('/').slice(-3).join('/')}\``);
lines.push('');
lines.push('---');
lines.push('');

const body = lines.join('\n');

if (process.argv.includes('--print')) { console.log(body); process.exit(0); }

if (!existsSync(OUT)) {
  writeFileSync(OUT, [
    '# SpacetimeDB build friction',
    '',
    'Appended after every SpacetimeDB run by the report command. The point is not the',
    'score — it is what the model fought with, since that is what SpacetimeDB can',
    'actually fix. Token counts are the CLI\'s own usage numbers, not estimates.',
    '',
    'Runs whose `run.json` says `contaminated: true` must not be read as evidence of',
    'anything: a build that read the marking scheme was not solving the same problem.',
    '',
    '---',
    '',
  ].join('\n'));
}
appendLocked(OUT, body);
console.log(`  stdb friction appended to ${OUT}`);
