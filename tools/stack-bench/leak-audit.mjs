#!/usr/bin/env node
// Did a generated build read anything it was not supposed to?
//
// Two earlier attempts at this got the answer wrong in opposite directions, so
// the failure modes are worth naming:
//
//   1. Counting only Read/Grep/Glob tool calls. A session that shells out to
//      `cat` or `grep` reads just as much and shows up as "0 reads" — which
//      reads as CLEAN when it means UNMEASURED.
//   2. Reconstructing the app directory from the transcript's folder name.
//      Hyphens in a run name become path separators, the boundary is nonsense,
//      and inside/outside is decided by a broken comparison.
//
// So: parse Bash commands as well as file tools, and take the app directory
// from the session's own cwd. Anything outside it is reported, whatever it is.
//
// Usage: node leak-audit.mjs [--dir <transcript-root>] [--json]

import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';

const arg = (n, d) => { const i = process.argv.indexOf(n); return i === -1 ? d : process.argv[i + 1]; };
const asJson = process.argv.includes('--json');
// --dir is the ONLY root when given. Scanning every project on the machine
// buries the runs under review in unrelated work.
//
// --app takes the application directory instead and finds the transcripts the
// CLI filed for it: they live under ~/.claude/projects in a folder whose name
// is the app's path with every separator and colon turned into a dash.
function transcriptsFor(appDir) {
  const base = join(homedir(), '.claude', 'projects');
  if (!existsSync(base)) return [];
  const want = resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase();
  return readdirSync(base)
    .filter(d => d.toLowerCase() === want || d.toLowerCase() === want.replace(/^-+/, ''))
    .map(d => join(base, d));
}

const ROOTS = arg('--app') ? transcriptsFor(arg('--app'))
  : arg('--dir') ? [resolve(arg('--dir'))]
  : [join(homedir(), '.claude', 'projects')];

const norm = s => String(s ?? '').replace(/\\/g, '/').replace(/^["']|["']$/g, '').toLowerCase();

// Files a build legitimately needs: its own app, plus node_modules noise.
const IGNORE = /node_modules|\.git[/\\]|package-lock\.json|\/dist\/|\.map$/;

// Commands that pull file contents into context.
const READER = /(?:^|[;&|]\s*)(?:cat|head|tail|less|more|type|grep|rg|ack|find|ls\s+-\w*l|sed\s+-n|awk)\s+([^;&|]+)/g;

const CLASSES = [
  [/\.claude[/\\]projects.*memory|[/\\]memory[/\\].*\.md$/, 'BENCHMARK NOTES'],
  [/scenarios[/\\].*\.json|grade\.mjs|mutation|check-scenarios/, 'GRADER / TEST SPECS'],
  [/contracts[/\\].*\.json|appendix-\d+\.md|walk\.mjs|lint\.mjs/, 'CONTRACT / LINTER'],
  [/prompts[/\\]|test-plans[/\\]|GRADING|RUBRIC/, 'PROMPTS / RUBRIC'],
  [/[/\\]skills[/\\]/, 'skill docs (intended)'],
  [/backends[/\\].*\.md|CLAUDE\.md|README/, 'setup docs (intended)'],
];
const classify = p => (CLASSES.find(([re]) => re.test(p)) ?? [null, 'other'])[1];

function sessionCwd(lines) {
  // The CLI records cwd on entries; take the most common one.
  const tally = new Map();
  for (const l of lines) {
    const m = l.match(/"cwd":"((?:[^"\\]|\\.)*)"/);
    if (!m) continue;
    const c = norm(m[1].replace(/\\\\/g, '/'));
    tally.set(c, (tally.get(c) ?? 0) + 1);
  }
  return [...tally].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

function pathsFromBash(cmd) {
  const out = [];
  for (const m of String(cmd).matchAll(READER)) {
    for (const tokRaw of m[1].split(/\s+/)) {
      const t = tokRaw.replace(/^["']|["']$/g, '');
      if (!t || t.startsWith('-')) continue;
      if (/[*?]/.test(t) || /\//.test(t) || /\\/.test(t) || /\.\w+$/.test(t)) out.push(t);
    }
  }
  return out;
}

function auditTranscript(file) {
  const lines = readFileSync(file, 'utf8').split('\n').filter(Boolean);
  const cwd = sessionCwd(lines);
  const hits = [];
  let fileTool = 0, bashReads = 0;

  for (const line of lines) {
    let e; try { e = JSON.parse(line); } catch { continue; }
    const c = e.message?.content;
    if (!Array.isArray(c)) continue;
    for (const p of c) {
      if (p.type !== 'tool_use') continue;
      const cand = [];
      if (/^(Read|Grep|Glob|NotebookRead)$/.test(p.name ?? '')) {
        fileTool++;
        cand.push(p.input?.file_path ?? p.input?.path ?? p.input?.pattern ?? '');
      } else if (p.name === 'Bash') {
        const found = pathsFromBash(p.input?.command ?? '');
        bashReads += found.length;
        cand.push(...found);
      }
      for (const raw of cand) {
        const n = norm(raw);
        if (!n || IGNORE.test(n)) continue;
        const absolute = /^[a-z]:/.test(n) || n.startsWith('/');
        if (!absolute) continue;              // relative paths resolve inside cwd
        if (cwd && n.startsWith(cwd)) continue;
        hits.push({ path: n, via: p.name, kind: classify(n) });
      }
    }
  }
  return { file, cwd, fileTool, bashReads, hits };
}

const results = [];
for (const root of ROOTS) {
  if (!existsSync(root)) continue;
  const stack = [root];
  while (stack.length) {
    const d = stack.pop();
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) { if (!/node_modules/.test(p)) stack.push(p); continue; }
      if (!/\.jsonl$/.test(e.name)) continue;
      if (!/transcript|^[0-9a-f-]{36}\.jsonl$/.test(e.name)) continue;
      if (statSync(p).size < 2000) continue;
      results.push({ ...auditTranscript(p), root });
    }
  }
}

if (asJson) { console.log(JSON.stringify(results, null, 2)); process.exit(0); }

const label = f => f.replace(/\\/g, '/').split('/').slice(-3).join('/').slice(0, 62);
console.log('\nBuilds that read outside their own directory');
console.log('(counts BOTH file tools and Bash cat/grep/find; boundary = the session\'s own cwd)\n');

let clean = 0;
for (const r of results.sort((a, b) => b.hits.length - a.hits.length)) {
  if (!r.cwd) { console.log(`  ?? ${label(r.file)} — no cwd recorded, cannot judge`); continue; }
  if (!r.hits.length) { clean++; continue; }
  const byKind = {};
  for (const h of r.hits) (byKind[h.kind] ??= []).push(h.path);
  console.log(`  ${label(r.file)}`);
  console.log(`      cwd: ...${r.cwd.slice(-52)}   (${r.fileTool} file-tool, ${r.bashReads} bash reads)`);
  for (const [k, v] of Object.entries(byKind).sort((a, b) => b[1].length - a[1].length)) {
    console.log(`      ${String(v.length).padStart(3)}x ${k.padEnd(22)} ${[...new Set(v)][0].split('/').slice(-2).join('/')}`);
  }
}
console.log(`\n  ${clean} transcript(s) read nothing outside their directory.`);
console.log(`  ${results.length} transcript(s) examined.\n`);
