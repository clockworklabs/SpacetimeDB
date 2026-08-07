#!/usr/bin/env node
// Where a build's context actually went.
//
// Cost is turns x context-per-turn, and context-per-turn is dominated by cache
// reads — so the question that matters is what the agent kept re-reading. The
// run summary can say a backend cost more; only the session transcript can say
// what it was carrying. This reads the CLI's own transcript for each generated
// app and attributes bytes to what produced them: the prompt, tool results,
// which files were read and how often.
//
// Usage: node token-analysis.mjs [--track ecommerce] [--run-index 0]

import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const arg = (n, d) => { const i = process.argv.indexOf(n); return i === -1 ? d : process.argv[i + 1]; };
const track = arg('--track', 'ecommerce');
const runIndex = arg('--run-index', '0');
const slug = track === 'chat' ? '' : (track === 'ecommerce' ? 'ecom' : track);

const PROJECTS = join(homedir(), '.claude', 'projects');
// ~4 characters per token is the usual rule of thumb for English + code. Every
// figure below is therefore an estimate, and labelled as one.
const tok = bytes => Math.round(bytes / 4);

function transcriptDirFor(backend) {
  const app = `results/${backend}${slug ? `-${slug}` : ''}-run${runIndex}/app`;
  const want = ('D--Development-ClockworkLabs-SpacetimeDB-SpacetimeDB-tools-stack-bench-'
    + app.replace(/[/\\]/g, '-')).toLowerCase();
  const hit = readdirSync(PROJECTS).find(d => d.toLowerCase() === want);
  return hit ? join(PROJECTS, hit) : null;
}

function analyse(backend) {
  const dir = transcriptDirFor(backend);
  if (!dir) return { backend, error: 'no transcript directory' };
  const files = readdirSync(dir).filter(f => f.endsWith('.jsonl'))
    .map(f => ({ f, m: statSync(join(dir, f)).mtimeMs }))
    .sort((a, b) => b.m - a.m).slice(0, 2);          // the build and the upgrade
  if (!files.length) return { backend, error: 'no transcripts' };

  const reads = new Map();        // path -> { count, bytes }
  const byKind = { toolResult: 0, assistant: 0, user: 0, other: 0 };
  let toolCalls = 0;

  for (const { f } of files) {
    for (const line of readFileSync(join(dir, f), 'utf8').split('\n')) {
      if (!line.trim()) continue;
      let e; try { e = JSON.parse(line); } catch { continue; }
      const s = JSON.stringify(e);

      const content = e.message?.content;
      if (Array.isArray(content)) {
        for (const part of content) {
          if (part.type === 'tool_use') {
            toolCalls++;
            const p = part.input?.file_path ?? part.input?.path ?? part.input?.notebook_path;
            if (p && /^(Read|NotebookRead)$/.test(part.name ?? '')) {
              const k = String(p).replace(/\\/g, '/');
              const cur = reads.get(k) ?? { count: 0, bytes: 0 };
              cur.count++;
              reads.set(k, cur);
            }
          }
          if (part.type === 'tool_result') {
            const text = typeof part.content === 'string' ? part.content : JSON.stringify(part.content ?? '');
            byKind.toolResult += text.length;
          }
          if (part.type === 'text') {
            byKind[e.type === 'assistant' ? 'assistant' : 'user'] += (part.text ?? '').length;
          }
        }
      } else byKind.other += s.length;
    }
  }

  // Attribute a read's cost by the file's size on disk now — the transcript
  // records that a file was read, not how big it was at the time.
  const appRoot = join('D:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDB/tools/stack-bench',
    `results/${backend}${slug ? `-${slug}` : ''}-run${runIndex}/app`);
  let readBytes = 0;
  const perFile = [];
  for (const [p, v] of reads) {
    let size = 0;
    try { size = statSync(p).size; } catch {
      try { size = statSync(join(appRoot, p)).size; } catch { size = 0; }
    }
    v.bytes = size * v.count;
    readBytes += v.bytes;
    perFile.push({ path: p, ...v, size });
  }
  perFile.sort((a, b) => b.bytes - a.bytes);
  return { backend, transcripts: files.length, toolCalls, byKind, readBytes, perFile };
}

const group = p => {
  const s = p.replace(/\\/g, '/');
  if (/module_bindings/.test(s)) return 'module_bindings (generated)';
  if (/\/client\//.test(s)) return 'client source';
  if (/\/(server|backend)\//.test(s)) return 'server source';
  if (/package(-lock)?\.json|tsconfig|vite\.config/.test(s)) return 'config';
  if (/stack-bench|BUG_REPORT|\.md$/.test(s)) return 'harness / docs';
  return 'other';
};

console.log(`\nWhat each build carried — ${track}, run ${runIndex}`);
console.log('(bytes from the CLI transcript; tokens estimated at ~4 bytes each)\n');

for (const backend of ['spacetime', 'postgres', 'mongodb']) {
  const r = analyse(backend);
  if (r.error) { console.log(`${backend}: ${r.error}\n`); continue; }
  console.log(`${backend.toUpperCase()}  (${r.transcripts} transcript(s), ${r.toolCalls} tool calls)`);
  console.log(`  tool results returned : ${(r.byKind.toolResult / 1024).toFixed(0).padStart(7)} KB  ~${tok(r.byKind.toolResult).toLocaleString()} tok`);
  console.log(`  assistant text        : ${(r.byKind.assistant / 1024).toFixed(0).padStart(7)} KB`);
  console.log(`  file reads (size x N) : ${(r.readBytes / 1024).toFixed(0).padStart(7)} KB  ~${tok(r.readBytes).toLocaleString()} tok`);

  const byGroup = {};
  for (const f of r.perFile) byGroup[group(f.path)] = (byGroup[group(f.path)] ?? 0) + f.bytes;
  for (const [g, b] of Object.entries(byGroup).sort((a, b) => b[1] - a[1])) {
    console.log(`      ${g.padEnd(28)} ${(b / 1024).toFixed(0).padStart(6)} KB`);
  }
  console.log('    most re-read files:');
  for (const f of r.perFile.slice(0, 5)) {
    console.log(`      ${String(f.count).padStart(3)}x  ${(f.size / 1024).toFixed(0).padStart(4)} KB  ${f.path.split('/').slice(-3).join('/')}`);
  }
  console.log();
}
