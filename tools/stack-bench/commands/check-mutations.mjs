#!/usr/bin/env node
// Verify that a mutation file can actually apply to the app it claims to
// target, WITHOUT running a graded pass.
//
// An anchor must match EXACTLY ONCE. Zero means the mutation is dead. More than
// one means the edit lands somewhere unintended, and a mutant that breaks the
// wrong location cannot provide qualification evidence for its declared check.
//
// Usage:
//   node commands/check-mutations.mjs --app <reference-app-dir> --mutations <file.json>
//   node commands/check-mutations.mjs --app <dir> --mutations <file.json> --quiet

import { readFileSync, existsSync } from 'node:fs';
import { mutationEdits, resolveMutationFile, validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';

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
const definitions = validateMutationDefinitions(spec.mutations);
for (const issue of definitions.issues) {
  console.log(`  BAD MANIFEST ${issue.mutation ?? '<unnamed>'} -> ${issue.kind}`);
  bad++;
}
for (const m of spec.mutations ?? []) {
  let file;
  try { file = resolveMutationFile(args.app, m.file); }
  catch {
    console.log(`  UNSAFE FILE ${m.id} -> ${m.file} escapes the app directory`);
    bad++;
    continue;
  }
  if (!existsSync(file)) {
    console.log(`  DEAD FILE   ${m.id} -> ${m.file} does not exist in this app`);
    bad++;
    continue;
  }
  const src = readFileSync(file, 'utf8');
  for (const e of mutationEdits(m)) {
    const n = src.split(e.find).length - 1;
    if (n === 1) { say(`  ok          ${m.id} -> ${m.file}`); continue; }
    console.log(n === 0
      ? `  DEAD ANCHOR ${m.id} -> not found in ${m.file}`
      : `  AMBIGUOUS   ${m.id} -> matches ${n}x in ${m.file}; the edit would land in more than one place`);
    bad++;
  }
}

console.log(bad
  ? `\n${bad} problem(s) — these mutations cannot validate anything against this app.`
  : `\nall anchors present and unique — this file can validate against this app.`);
process.exit(bad ? 1 : 0);
