#!/usr/bin/env node
// Use the recorded cwd as the app boundary; transcript folder names are not authority.

import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { CODING_CONTAINER_APP_ROOT } from '../src/runtime/coding-container-policy.js';

// --dir is exclusive. --app resolves its matching CLI transcript directory.
function transcriptsFor(appDir: string): string[] {
  const base = join(homedir(), '.claude', 'projects');
  if (!existsSync(base)) return [];
  const want = resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase();
  return readdirSync(base)
    .filter(d => d.toLowerCase() === want || d.toLowerCase() === want.replace(/^-+/, ''))
    .map(d => join(base, d));
}

const norm = (value: unknown): string => String(value ?? '')
  .replace(/\\/g, '/').replace(/^["']|["']$/g, '').toLowerCase();

// Ignore dependencies, build output, and this session's CLI task output.
const IGNORE = /node_modules|\.git[/\\]|package-lock\.json|\/dist\/|\.map$|[/\\]temp[/\\]claude[/\\].*[/\\]tasks[/\\]/;

// Commands that pull file contents into context.
const READER = /(?:^|[;&|]\s*)(?:cat|head|tail|less|more|type|grep|rg|ack|find|ls\s+-\w*l|sed\s+-n|awk)\s+([^;&|]+)/g;

const CLASSES: Array<readonly [RegExp, string]> = [
  [/[/\\]stack-bench(?:[/\\]|$)/, 'GRADER / TEST SPECS'],
  [/\.claude[/\\]projects.*memory|[/\\]memory[/\\].*\.md$/, 'BENCHMARK NOTES'],
  [/scenarios[/\\].*\.json|grade\.(?:js|ts)|mutation|check-scenarios/, 'GRADER / TEST SPECS'],
  [/contracts[/\\].*\.json|appendix-\d+\.md|walk\.(?:js|ts)|lint\.(?:js|ts)/, 'CONTRACT / LINTER'],
  [/prompts[/\\]|test-plans[/\\]|GRADING|RUBRIC/, 'PROMPTS / RUBRIC'],
  [/[/\\]skills[/\\]/, 'skill docs (intended)'],
  [/backends[/\\].*\.md|CLAUDE\.md|README/, 'setup docs (intended)'],
];
const classify = (path: string): string => CLASSES.find(([pattern]) => pattern.test(path))?.[1]
  ?? 'other';

// The shallowest recorded cwd is the app boundary when --app is absent.
function sessionCwd(lines: string[]): string | null {
  const seen = new Set<string>();
  for (const l of lines) {
    const m = l.match(/"cwd":"((?:[^"\\]|\\.)*)"/);
    if (m?.[1]) seen.add(norm(m[1].replace(/\\\\/g, '/')));
  }
  if (!seen.size) return null;
  return [...seen].sort((a, b) => a.split('/').length - b.split('/').length || a.length - b.length)[0]
    ?? null;
}

export function pathsFromBash(command: unknown): string[] {
  const out: string[] = [];
  for (const match of String(command).matchAll(READER)) {
    const argumentsText = match[1];
    if (!argumentsText) continue;
    for (const tokRaw of argumentsText.split(/\s+/)) {
      const t = tokRaw.replace(/^["']|["']$/g, '');
      if (!t || t.startsWith('-')) continue;
      if (/[*?]/.test(t) || /\//.test(t) || /\\/.test(t) || /\.\w+$/.test(t)) out.push(t);
    }
  }
  return out;
}

// Count file-tool reads only after their result confirms success.
// Bash reads count unless the command fails.
interface AuditHit {
  path: string;
  via: string;
  kind: string;
  unresolved?: boolean;
}

interface PendingRead {
  paths: string[];
  via: string;
}

interface TranscriptAudit {
  file: string;
  cwd: string | null;
  fileTool: number;
  bashReads: number;
  hits: AuditHit[];
  refused: AuditHit[];
}

interface AuditResult extends TranscriptAudit {
  root: string;
}

interface TranscriptContent {
  type?: string;
  name?: string;
  id?: string;
  tool_use_id?: string;
  is_error?: boolean;
  input?: { file_path?: string; path?: string; pattern?: string; command?: string };
}

export function auditTranscript(file: string, boundary: string | null): TranscriptAudit {
  const lines = readFileSync(file, 'utf8').split('\n').filter(Boolean);
  // Container transcripts use /app; --app is its host path.
  const own = sessionCwd(lines);
  const cwd = own === CODING_CONTAINER_APP_ROOT ? own : (boundary ?? own);
  const hits: AuditHit[] = [];
  const refused: AuditHit[] = [];
  const pending = new Map<string, PendingRead>();
  let fileTool = 0, bashReads = 0;

  for (const line of lines) {
    let event: { message?: { content?: TranscriptContent[] } };
    try { event = JSON.parse(line) as typeof event; } catch { continue; }
    const c = event.message?.content;
    if (!Array.isArray(c)) continue;
    for (const p of c) {
      if (p.type === 'tool_result' && p.tool_use_id && pending.has(p.tool_use_id)) {
        const completed = pending.get(p.tool_use_id ?? '');
        if (!completed) continue;
        const { paths, via } = completed;
        pending.delete(p.tool_use_id ?? '');
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
        let n = norm(raw);
        if (!n || IGNORE.test(n)) continue;
        // The CLI keeps auto-memory for the session's OWN project dir. A build
        // A session may read its own memory, never another project's memory.
        if (cwd && /[/\\]projects[/\\][^/\\]+[/\\]memory[/\\]/.test(n)
          && n.includes(cwd.replace(/[\\/:]/g, '-'))) continue;
        const absolute = /^[a-z]:/.test(n) || n.startsWith('/');
        if (!absolute && cwd) n = `${cwd}/${n.replace(/^\.\//, '')}`;
        const privateHarnessPath = cwd
          && (n === `${cwd}/stack-bench` || n.startsWith(`${cwd}/stack-bench/`));
        if (!privateHarnessPath && !absolute) continue;
        if (!privateHarnessPath && cwd && (n === cwd || n.startsWith(`${cwd}/`))) continue;
        paths.push(n);
      }
      if (paths.length && p.id && p.name) pending.set(p.id, { paths, via: p.name });
    }
  }
  // A call whose result never arrived (session cut short) is unresolved, and
  // unresolved is not innocent: count it.
  for (const { paths, via } of pending.values())
    for (const n of paths) hits.push({ path: n, via, kind: classify(n), unresolved: true });

  return { file, cwd, fileTool, bashReads, hits, refused };
}

function main(): void {
  const { values } = parseArgs({ args: process.argv.slice(2), options: {
    app: { type: 'string' }, dir: { type: 'string' }, json: { type: 'boolean' },
  } });
  const requestedApp = values.app;
  const requestedDirectory = values.dir;
  if (requestedApp && requestedDirectory) throw new Error('--app and --dir cannot be used together');
  const roots = requestedApp ? transcriptsFor(requestedApp)
    : requestedDirectory ? [resolve(requestedDirectory)]
    : [join(homedir(), '.claude', 'projects')];
  // When the caller names the app directory, that is the boundary. Do not
  // infer it from a transcript folder name.
  const appBoundary = requestedApp ? norm(resolve(requestedApp)) : null;
  const results: AuditResult[] = [];
for (const root of roots) {
  if (!existsSync(root)) continue;
  const stack = [root];
  while (stack.length) {
    const d = stack.pop();
    if (!d) continue;
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) { if (!/node_modules/.test(p)) stack.push(p); continue; }
      if (!/\.jsonl$/.test(e.name)) continue;
      // Include transcripts from the main session and its subagents.
      if (!/transcript|^agent-|^[0-9a-f-]{36}\.jsonl$/.test(e.name)) continue;
      if (statSync(p).size < 2000) continue;
      results.push({ ...auditTranscript(p, appBoundary), root });
    }
  }
}

if (values.json) {
  console.log(JSON.stringify(results, null, 2));
  return;
}

const label = (file: string): string => file.replace(/\\/g, '/')
  .split('/').slice(-3).join('/').slice(0, 62);
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
  const byKind: Record<string, string[]> = {};
  for (const h of r.hits) (byKind[h.kind] ??= []).push(h.path);
  console.log(`  ${label(r.file)}`);
  console.log(`      cwd: ...${r.cwd.slice(-52)}   (${r.fileTool} file-tool, ${r.bashReads} bash reads)`);
  for (const [k, v] of Object.entries(byKind).sort((a, b) => b[1].length - a[1].length)) {
    const example = [...new Set(v)][0] ?? '';
    console.log(`      ${String(v.length).padStart(3)}x ${k.padEnd(22)} ${example.split('/').slice(-2).join('/')}`);
  }
}
console.log(`\n  ${clean} transcript(s) read nothing outside their directory.`);
console.log(`  ${results.length} transcript(s) examined.\n`);
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) main();
