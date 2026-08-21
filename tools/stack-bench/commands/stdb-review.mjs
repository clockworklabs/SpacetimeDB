#!/usr/bin/env node
// Where did SpacetimeDB cost the model something the other backends did not?
//
// stdb-report.mjs counts things that announced themselves as errors. Three
// kinds of friction never do:
//
//   1. Sequence. "edit schema -> publish fails -> edit -> fails -> edit -> works"
//      is ONE struggle with a shape; counted as errors it is just "2".
//   2. Workarounds. The model settling for a worse pattern because the good one
//      was hard. Nothing fails, so nothing is recorded.
//   3. Misuse that SUCCEEDED — a full table scan where an index existed. This is
//      the most valuable of the three and the most invisible: the build passes.
//
// Reasoning would be the obvious place to look for these and it is NOT
// available: `thinking` blocks arrive with `thinking: ""` and a signature, in
// every output mode including --output-format stream-json
// --include-partial-messages. Verified, not assumed. So this reads BEHAVIOUR —
// the ordered tool calls and their outcomes — which is a better record anyway,
// since actions cannot be rationalised after the fact.
//
// EVERY finding must quote the log verbatim, and every quote is checked against
// the log before it is written down. A model asked for friction will produce
// friction. Today alone three oracles in this harness reported success they had
// not earned — a deny list that enforced nothing, a probe that matched the word
// "denied" inside the file it had just read, an audit that counted blocked
// attempts as leaks — and each looked fine until it was run against a known
// answer. An unverifiable finding here would send someone to fix a bug that
// never happened.
//
// Usage:
//   node commands/stdb-review.mjs --label spacetime-ecom-run0
//                        [--compare postgres-ecom-run0,mongodb-ecom-run0]
//                        [--source <dir>] [--model claude-sonnet-5] [--print]

import { readFileSync, readdirSync, existsSync, appendFileSync, statSync, openSync as fsOpenSync, closeSync, rmSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, dirname, resolve } from 'node:path';

import { operationalOutputRoot } from '../src/runtime/operational-paths.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';
const OPERATIONAL_ROOT = operationalOutputRoot(ROOT);

// Concurrent runs append here: n=5 isolated trials all write one friction
// record, and interleaved appends can split an entry down the middle. An
// exclusive-create lock serialises them; a stale lock older than a minute is
// broken rather than deadlocking a benchmark run over a log file.
function appendLocked(file, text) {
  const lock = file + '.lock';
  for (let i = 0; i < 120; i++) {
    try {
      const fd = fsOpenSync(lock, 'wx');
      try { appendFileSync(file, text); } finally { closeSync(fd); rmSync(lock, { force: true }); }
      return;
    } catch (e) {
      if (e.code !== 'EEXIST') throw e;
      try {
        if (Date.now() - statSync(lock).mtimeMs > 60000) rmSync(lock, { force: true });
      } catch { /* someone else cleared it */ }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 250);
    }
  }
  appendFileSync(file, text);   // lock never freed: a garbled entry beats a lost one
}

const arg = (n, d) => { const i = process.argv.indexOf(n); return i === -1 ? d : process.argv[i + 1]; };

const label = arg('--label');
if (!label) { console.error('need --label <transcripts folder>'); process.exit(2); }
const OUT = resolve(OPERATIONAL_ROOT, arg('--out', 'local-notes/STDB-FRICTION.md'));
mkdirSync(dirname(OUT), { recursive: true });

function findClaude() {
  const appData = process.env.APPDATA ?? join(process.env.HOME ?? '', 'AppData', 'Roaming');
  const desktop = join(appData, 'Claude', 'claude-code');
  if (existsSync(desktop)) {
    const versions = readdirSync(desktop).sort();
    const exe = join(desktop, versions[versions.length - 1], 'claude.exe');
    if (existsSync(exe)) return exe;
  }
  return 'claude';
}

// ── Distilling a run into an action log ────────────────────────────────────

// Whole transcripts are megabytes of file contents the model already knows it
// wrote. What carries signal is the ORDER of actions and what came back from
// the ones that did not work, so results are kept short and successful reads
// are dropped entirely.
function actionLog(dir, sinceMs) {
  if (!existsSync(dir)) return { log: '', turns: 0 };
  const files = readdirSync(dir).filter(f => f.endsWith('.jsonl'))
    .map(f => ({ f, m: statSync(join(dir, f)).mtimeMs }))
    .sort((a, b) => a.m - b.m)
    .filter(f => f.m >= sinceMs);

  const out = [];
  let turn = 0;
  for (const { f } of files) {
    out.push(`--- session ${f.slice(0, 8)} ---`);
    const pending = new Map();
    for (const line of readFileSync(join(dir, f), 'utf8').split('\n')) {
      if (!line.trim()) continue;
      let e; try { e = JSON.parse(line); } catch { continue; }
      const content = e.message?.content;
      if (!Array.isArray(content)) continue;
      for (const p of content) {
        if (p.type === 'text' && e.type === 'assistant' && p.text?.trim()) {
          out.push(`[${++turn}] SAYS: ${p.text.replace(/\s+/g, ' ').trim().slice(0, 400)}`);
        } else if (p.type === 'tool_use') {
          const target = String(p.input?.command ?? p.input?.file_path ?? p.input?.pattern ?? '')
            .replace(/\s+/g, ' ').trim().slice(0, 160);
          pending.set(p.id, { name: p.name, target, turn: ++turn });
          out.push(`[${turn}] ${p.name}: ${target}`);
        } else if (p.type === 'tool_result' && pending.has(p.tool_use_id)) {
          const meta = pending.get(p.tool_use_id);
          pending.delete(p.tool_use_id);
          const body = typeof p.content === 'string' ? p.content : JSON.stringify(p.content ?? '');
          const bad = p.is_error === true
            || /error TS\d+|Pre-publish check failed|Aborting because|npm ERR!|not assignable|Cannot find/i.test(body);
          // A successful read tells us nothing; a failure tells us everything.
          if (bad) out.push(`      -> FAILED: ${body.replace(/\s+/g, ' ').trim().slice(0, 320)}`);
          else if (/^(Bash|PowerShell)$/.test(meta.name)) {
            out.push(`      -> ok: ${body.replace(/\s+/g, ' ').trim().slice(0, 120)}`);
          }
        }
      }
    }
  }
  return { log: out.join('\n'), turns: turn };
}

const dir = join(OPERATIONAL_ROOT, 'transcripts', label);
if (!existsSync(dir)) { console.error(`no archived transcripts at ${dir}`); process.exit(2); }
const files = readdirSync(dir).filter(f => f.endsWith('.jsonl'))
  .map(f => statSync(join(dir, f)).mtimeMs).sort((a, b) => a - b);
const sinceMs = Number(arg('--since-ms', 0)) || (files.length ? files[files.length - 1] - 6 * 3600_000 : 0);

const mine = actionLog(dir, sinceMs);
if (!mine.turns) { console.error('nothing to review'); process.exit(2); }

// The question is comparative — "falling behind" needs something to be behind.
// The other backends' logs are included in outline so the reviewer can say what
// SpacetimeDB needed that they did not.
const compare = [];
for (const other of String(arg('--compare', '')).split(',').filter(Boolean)) {
  const transcriptRoot = join(OPERATIONAL_ROOT, 'transcripts');
  const exact = join(transcriptRoot, other);
  const latest = existsSync(transcriptRoot)
    ? readdirSync(transcriptRoot)
      .filter(name => name.startsWith(`${other}-`) && statSync(join(transcriptRoot, name)).isDirectory())
      .sort((a, b) => statSync(join(transcriptRoot, b)).mtimeMs - statSync(join(transcriptRoot, a)).mtimeMs)[0]
    : null;
  const d = latest ? join(transcriptRoot, latest) : exact;
  if (!existsSync(d)) continue;
  const c = actionLog(d, 0);
  compare.push(`### ${other} (${c.turns} actions)\n${c.log.slice(0, 25_000)}`);
}

// The shipped source answers the question errors cannot: was the API used well?
let source = '';
const srcDir = arg('--source');
if (srcDir && existsSync(srcDir)) {
  const stack = [srcDir];
  const picked = [];
  while (stack.length && picked.length < 12) {
    const d = stack.pop();
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) { if (!/node_modules|dist|module_bindings/.test(e.name)) stack.push(p); continue; }
      if (/\.(ts|tsx)$/.test(e.name) && statSync(p).size < 60_000) picked.push(p);
    }
  }
  source = picked.map(p => `#### ${p.split(/[\\/]/).slice(-2).join('/')}\n${readFileSync(p, 'utf8').slice(0, 12_000)}`)
    .join('\n\n').slice(0, 90_000);
}

// ── Asking, with the answer constrained to quotable evidence ───────────────

const SCHEMA = `{"findings":[{"title":"short","surface":"server API|client SDK|CLI/publish|generated bindings|docs|other","cost":"what it cost — attempts, retries, or a worse implementation","evidence":"VERBATIM line copied from the log or source","fix":"what SpacetimeDB could change"}]}`;

const prompt = [
  'You are reviewing how a coding model built an application on SpacetimeDB, to find',
  'what SpacetimeDB should improve. You are NOT judging the application.',
  '',
  'Report only friction attributable to SpacetimeDB itself: its server API, client SDK,',
  'CLI, generated bindings, or documentation. Ignore anything caused by the test harness,',
  'the model, or the other backends. Look especially for:',
  '  - repeated cycles against the same file or command (the same thing fixed twice)',
  '  - workarounds adopted after something proved awkward',
  '  - API used in a way that WORKED but is wrong or wasteful (this never errors)',
  '',
  'RULES:',
  '  - Every finding MUST include "evidence": a single line copied VERBATIM from the',
  '    material below. Do not paraphrase, do not join lines, do not add ellipses.',
  '    Findings whose evidence cannot be found verbatim are discarded automatically.',
  '  - If you find nothing well-evidenced, return {"findings":[]}. An empty answer is',
  '    correct and useful; an invented one wastes engineering time.',
  '',
  `Reply with JSON only, matching: ${SCHEMA}`,
  '',
  '## SpacetimeDB action log',
  mine.log.slice(0, 160_000),
  ...(compare.length ? ['', '## Other backends, for contrast', ...compare] : []),
  ...(source ? ['', '## Shipped SpacetimeDB source', source] : []),
].join('\n');

let raw = '';
try {
  raw = execFileSync(findClaude(), ['--print', '--output-format', 'text',
    '--permission-mode', 'acceptEdits', '--model', arg('--model', 'claude-sonnet-5')],
    { input: prompt, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
} catch (e) {
  console.error(`review session failed: ${String(e.message).split('\n')[0]}`);
  process.exit(1);
}

let findings = [];
try {
  const m = raw.match(/\{[\s\S]*\}/);
  findings = JSON.parse(m ? m[0] : raw).findings ?? [];
} catch {
  // Say what came back instead. "Did not return usable JSON" with no sample is
  // a dead end for whoever has to fix it.
  console.error('review did not return usable JSON. It replied:\n'
    + raw.slice(0, 600).replace(/^/gm, '  '));
  process.exit(1);
}

// ── Verification: a quote that is not in the log did not happen ────────────

const haystack = (mine.log + '\n' + compare.join('\n') + '\n' + source).replace(/\s+/g, ' ');
const norm = s => String(s ?? '').replace(/\s+/g, ' ').trim();
const verified = [], rejected = [];
for (const f of findings) {
  const q = norm(f.evidence);
  // Short quotes match by accident; a fabricated finding should not slip
  // through on a fragment like "error".
  if (q.length >= 24 && haystack.includes(q)) verified.push(f);
  else rejected.push({ ...f, why: q.length < 24 ? 'quote too short to verify' : 'quote not found in the log' });
}

const lines = [];
lines.push(`**Behavioural review** — ${verified.length} finding(s) with verified evidence`
  + `${rejected.length ? `, ${rejected.length} discarded as unverifiable` : ''}`);
lines.push('');
for (const f of verified) {
  lines.push(`- **${f.title}** *(${f.surface})*`);
  lines.push(`  - cost: ${f.cost}`);
  lines.push(`  - evidence: \`${norm(f.evidence).slice(0, 220).replace(/`/g, "'")}\``);
  if (f.fix) lines.push(`  - possible fix: ${f.fix}`);
}
if (!verified.length) lines.push('- nothing survived verification this run.');
if (rejected.length) {
  lines.push('');
  lines.push(`<sub>Discarded (evidence not found verbatim): ${rejected.map(r => r.title).join('; ')}</sub>`);
}
lines.push('');
lines.push('---');
lines.push('');

const body = lines.join('\n');
if (process.argv.includes('--print')) { console.log(body); process.exit(0); }
appendLocked(OUT, body);
console.log(`  behavioural review appended to ${OUT} (${verified.length} verified, ${rejected.length} discarded)`);
