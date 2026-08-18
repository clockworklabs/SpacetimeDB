#!/usr/bin/env node
// Copy each run's session transcript out of the CLI's store and into durable
// operational storage. A source checkout keeps the historical repo-local
// layout; the read-only appliance uses STACK_BENCH_RESULTS_DIR.
//
// The transcripts are the only record of what a build actually read, so they
// are the whole evidence base for the contamination audit — a score is only
// trustworthy because leak-audit.mjs could examine them. They live under
// ~/.claude/projects, which the CLI prunes on a timer (cleanupPeriodDays,
// 30 by default), and nothing else keeps a copy.
//
// That has already cost us a set: the June sequential runs kept their OTel
// telemetry but not their transcripts, the CLI deleted its own copies around
// mid-July, and those runs are now permanently unauditable. Telemetry is no
// substitute — it records that a Read happened and never what was read.
//
// Usage: node archive-transcripts.mjs [--results <dir>] [--out <dir>]

import { readdirSync, existsSync, copyFileSync, mkdirSync, statSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { operationalOutputRoot } from '../src/runtime/operational-paths.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';
const OPERATIONAL_ROOT = operationalOutputRoot(ROOT);
const arg = (n, d) => { const i = process.argv.indexOf(n); return i === -1 ? d : process.argv[i + 1]; };

const RESULTS = resolve(arg('--results', join(ROOT, 'results')));
const OUT = resolve(arg('--out', join(OPERATIONAL_ROOT, 'transcripts')));
const STORE = join(homedir(), '.claude', 'projects');

// The CLI files a session under a folder named for the directory it ran in,
// with every separator and colon flattened to a dash.
const encode = p => resolve(p).replace(/[\\/:]/g, '-').toLowerCase();

function storeDirFor(appDir) {
  if (!existsSync(STORE)) return null;
  const want = encode(appDir);
  const hit = readdirSync(STORE).find(d => {
    const n = d.toLowerCase();
    return n === want || n === want.replace(/^-+/, '');
  });
  return hit ? join(STORE, hit) : null;
}

// A run builds in an isolated work directory outside the harness, so the CLI
// files its transcript under THAT path, not under results/. Sweeping results/
// would quietly archive nothing. --app names the directory the build actually
// ran in; --label names the folder to file it under.
const APP = arg('--app');
const LABEL = arg('--label', APP ? 'run' : null);

const jobs = [];
if (APP) {
  jobs.push([LABEL, resolve(APP)]);
} else {
  if (!existsSync(RESULTS)) {
    console.error(`no results directory at ${RESULTS}`);
    process.exit(1);
  }
  for (const run of readdirSync(RESULTS, { withFileTypes: true })) {
    if (!run.isDirectory()) continue;
    const appDir = join(RESULTS, run.name, 'app');
    if (existsSync(appDir)) jobs.push([run.name, appDir]);
  }
}
mkdirSync(OUT, { recursive: true });

let copied = 0, missing = 0;
for (const [name, appDir] of jobs) {
  const run = { name };

  const store = storeDirFor(appDir);
  if (!store) {
    console.log(`  ${run.name}: NO TRANSCRIPT in the CLI store — already pruned, or never run`);
    missing++;
    continue;
  }

  const dest = join(OUT, run.name);
  mkdirSync(dest, { recursive: true });
  // Subagent transcripts live at <store>/<sessionId>/subagents/agent-*.jsonl.
  // Copying only the top level left them behind, and with them their cost: a
  // SpacetimeDB run reconciled $1.44 short until they were found by hand. A
  // session that spawns help is not cheaper for it.
  const collect = (dir, out = []) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) collect(p, out);
      else if (e.name.endsWith('.jsonl')) out.push(p);
    }
    return out;
  };
  for (const abs of collect(store)) {
    const f = abs.slice(store.length + 1).replace(/[\/]/g, '__');
    const from = abs, to = join(dest, f);
    // Re-running must not silently overwrite an archived copy with a shorter
    // live one; the archive is the record of last resort.
    if (existsSync(to) && statSync(to).size >= statSync(from).size) continue;
    copyFileSync(from, to);
    copied++;
    console.log(`  ${run.name}/${f}  (${(statSync(from).size / 1024 / 1024).toFixed(1)} MB)`);
  }
}

console.log(`\n${copied} transcript(s) archived to ${OUT}`);
if (missing) console.log(`${missing} run(s) had no transcript left to archive.`);
