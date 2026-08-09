#!/usr/bin/env node
// Verify that a mutation file can actually apply to the app it claims to
// target, WITHOUT running a graded pass.
//
// This exists because the failure it catches is silent and total. The
// ecommerce PostgreSQL mutations pointed at `server/src/index.ts` and a table
// called `item_stock`; the reference build has `server/src/stockOps.js` and a
// table called `stock`. Not one anchor matched, so every mutation failed to
// apply, every mutant read as unbroken, and no contention criterion could ever
// clear the promotion gate. Nothing in the pipeline said so.
//
// An anchor must match EXACTLY ONCE. Zero means the mutation is dead. More than
// one means the edit lands somewhere unintended, and a mutant that breaks the
// wrong thing is worse than none — it promotes a criterion for catching a
// defect it was not written to catch.
//
// Usage:
//   node check-mutations.mjs --app <reference-app-dir> --mutations mutations/<file>.json
//   node check-mutations.mjs --app <dir> --mutations <file> --quiet   (exit code only)

import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') a.app = argv[++i];
    else if (argv[i] === '--mutations') a.mutations = argv[++i];
    else if (argv[i] === '--quiet') a.quiet = true;
    else { console.error(`Unknown arg ${argv[i]}`); process.exit(2); }
  }
  if (!a.app || !a.mutations) {
    console.error('Usage: node check-mutations.mjs --app <dir> --mutations <file.json>');
    process.exit(2);
  }
  return a;
}

const args = parseArgs(process.argv);
const spec = JSON.parse(readFileSync(args.mutations, 'utf8'));
const say = (...m) => { if (!args.quiet) console.log(...m); };

say(`mutations : ${args.mutations}`);
say(`app       : ${args.app}`);
if (spec.anchoredTo) say(`anchored  : ${String(spec.anchoredTo).split('.')[0]}`);
say('');

let bad = 0;
for (const m of spec.mutations ?? []) {
  const file = join(args.app, m.file);
  if (!existsSync(file)) {
    console.log(`  DEAD FILE   ${m.id} -> ${m.file} does not exist in this app`);
    bad++;
    continue;
  }
  const src = readFileSync(file, 'utf8');
  for (const e of m.edits ?? []) {
    const n = src.split(e.find).length - 1;
    if (n === 1) { say(`  ok          ${m.id} -> ${m.file}`); continue; }
    console.log(n === 0
      ? `  DEAD ANCHOR ${m.id} -> not found in ${m.file}`
      : `  AMBIGUOUS   ${m.id} -> matches ${n}x in ${m.file}; the edit would land in more than one place`);
    bad++;
  }
  // A mutation that names no criterion cannot promote anything, and a `breaks`
  // that is not a numeric feature id makes mutation-test.mjs score `undefined`
  // and report SURVIVED for a mutant that worked perfectly.
  if (typeof m.breaks !== 'number') {
    console.log(`  BAD BREAKS  ${m.id} -> "breaks" must be the numeric feature id, got ${JSON.stringify(m.breaks)}`);
    bad++;
  }
}

console.log(bad
  ? `\n${bad} problem(s) — these mutations cannot validate anything against this app.`
  : `\nall anchors present and unique — this file can validate against this app.`);
process.exit(bad ? 1 : 0);
