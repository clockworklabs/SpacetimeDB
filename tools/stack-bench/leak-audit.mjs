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

// When the caller names the app directory, that IS the boundary — no guessing
// from whatever directory the session happened to be standing in.
const APP_BOUNDARY = arg('--app') ? norm(resolve(arg('--app'))) : null;

// Files a build legitimately needs: its own app, plus node_modules noise.
// ...plus the CLI's own scratch for THIS session (background task output lives
// under temp/claude/<encoded-app>/<session>/tasks). That is the build reading
// its own command output, not the harness.
const IGNORE = /node_modules|\.git[/\\]|package-lock\.json|\/dist\/|\.map$|[/\\]temp[/\\]claude[/\\].*[/\\]tasks[/\\]/;

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

// The boundary is the APP directory, not wherever the session happened to be
// standing. Taking the most common cwd looked reasonable and was wrong: a
// SpacetimeDB build spends most of its turns in backend/spacetimedb, so reads of
// its OWN client/src/module_bindings/*.ts resolved as escapes and the run was
// reported contaminated by its own generated bindings.
//
// When --app names the directory, that is the answer. Otherwise take the
// SHALLOWEST cwd seen, which is the closest thing to the app root the transcript
// knows about — never the most frequent.
function sessionCwd(lines) {
  const seen = new Set();
  for (const l of lines) {
    const m = l.match(/"cwd":"((?:[^"\\]|\\.)*)"/);
    if (m) seen.add(norm(m[1].replace(/\\\\/g, '/')));
  }
  if (!seen.size) return null;
  return [...seen].sort((a, b) => a.split('/').length - b.split('/').length || a.length - b.length)[0];
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

// An ATTEMPT is not a leak. Once the sandbox actually refuses things, a build
// that tries to read the rubric and is blocked looks identical to one that
// succeeded — and marking the blocked run contaminated would void exactly the
// runs the sandbox is protecting. So a candidate path is held against its
// tool_use id and only counted once the result comes back not-an-error.
//
// Bash is not governed by the Read rules, so its reads resolve as successful
// unless the command itself failed; that asymmetry is the point of auditing it.
function auditTranscript(file, boundary) {
  const lines = readFileSync(file, 'utf8').split('\n').filter(Boolean);
  const cwd = boundary ?? sessionCwd(lines);
  const hits = [], refused = [];
  const pending = new Map();
  let fileTool = 0, bashReads = 0;

  for (const line of lines) {
    let e; try { e = JSON.parse(line); } catch { continue; }
    const c = e.message?.content;
    if (!Array.isArray(c)) continue;
    for (const p of c) {
      if (p.type === 'tool_result' && pending.has(p.tool_use_id)) {
        const { paths, via } = pending.get(p.tool_use_id);
        pending.delete(p.tool_use_id);
        const blocked = p.is_error === true;
        for (const n of paths) (blocked ? refused : hits).push({ path: n, via, kind: classify(n) });
        continue;
      }
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
      const paths = [];
      for (const raw of cand) {
        const n = norm(raw);
        if (!n || IGNORE.test(n)) continue;
        const absolute = /^[a-z]:/.test(n) || n.startsWith('/');
        if (!absolute) continue;              // relative paths resolve inside cwd
        if (cwd && n.startsWith(cwd)) continue;
        paths.push(n);
      }
      if (paths.length) pending.set(p.id, { paths, via: p.name });
    }
  }
  // A call whose result never arrived (session cut short) is unresolved, and
  // unresolved is not innocent: count it.
  for (const { paths, via } of pending.values())
    for (const n of paths) hits.push({ path: n, via, kind: classify(n), unresolved: true });

  return { file, cwd, fileTool, bashReads, hits, refused };
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
      results.push({ ...auditTranscript(p, APP_BOUNDARY), root });
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
  if (!r.hits.length) {
    clean++;
    // Blocked attempts are worth printing: they are the sandbox doing its job,
    // and they say which paths a build still goes looking for.
    if (r.refused?.length) {
      const kinds = [...new Set(r.refused.map(h => h.kind))].join(', ');
      console.log(`  ${label(r.file)}\n      clean — ${r.refused.length} attempt(s) BLOCKED by the sandbox (${kinds})`);
    }
    continue;
  }
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
