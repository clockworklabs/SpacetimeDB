#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
import { parseArgs as parseNodeArgs } from 'node:util';

import { mutationFileEdits, resolveMutationFile, validateMutationDefinitions }
  from '../src/evidence/mutation-analysis.js';
import type { MutationDefinition } from '../src/evidence/mutation-analysis.js';

interface CliArgs {
  app: string;
  mutations: string;
  quiet: boolean;
}

interface MutationSpec {
  anchoredTo?: unknown;
  mutations?: MutationDefinition[];
}

function parseArgs(argv: string[]): CliArgs {
  const { values: { app, mutations, quiet = false } } = parseNodeArgs({ args: argv.slice(2),
    options: { app: { type: 'string' }, mutations: { type: 'string' }, quiet: { type: 'boolean' } } });
  if (!app || !mutations) {
    console.error('Usage: node dist/commands/check-mutations.js --app <dir> --mutations <file.json>');
    process.exit(2);
  }
  return { app, mutations, quiet };
}

const args = parseArgs(process.argv);
const parsed: unknown = JSON.parse(readFileSync(args.mutations, 'utf8'));
if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
  throw new Error('mutation manifest must be an object');
}
const spec = parsed as MutationSpec;
const say = (...message: unknown[]): void => { if (!args.quiet) console.log(...message); };

say(`mutations : ${args.mutations}`);
say(`app       : ${args.app}`);
if (spec.anchoredTo) say(`anchored  : ${String(spec.anchoredTo).split('.')[0]}`);
say('');

let bad = 0;
const definitions = validateMutationDefinitions(spec.mutations);
for (const issue of definitions.issues) {
  console.log(`  BAD MANIFEST ${issue.mutation ?? '<unnamed>'} -> ${issue.kind}`);
  bad += 1;
}
for (const mutation of spec.mutations ?? []) {
  const mutationId = String(mutation.id ?? '<unnamed>');
  for (const edit of mutationFileEdits(mutation)) {
    let file: string;
    try { file = resolveMutationFile(args.app, edit.file); }
    catch {
      console.log(`  UNSAFE FILE ${mutationId} -> ${edit.file} escapes the app directory`);
      bad += 1;
      continue;
    }
    if (!existsSync(file)) {
      console.log(`  DEAD FILE   ${mutationId} -> ${edit.file} does not exist in this app`);
      bad += 1;
      continue;
    }
    const source = readFileSync(file, 'utf8');
    const matches = source.split(edit.find).length - 1;
    if (matches === 1) {
      say(`  ok          ${mutationId} -> ${edit.file}`);
      continue;
    }
    console.log(matches === 0
      ? `  DEAD ANCHOR ${mutationId} -> not found in ${edit.file}`
      : `  AMBIGUOUS   ${mutationId} -> matches ${matches}x in ${edit.file}; the edit would land in more than one place`);
    bad += 1;
  }
}

console.log(bad
  ? `\n${bad} problem(s) — these mutations cannot validate anything against this app.`
  : '\nall anchors present and unique — this file can validate against this app.');
process.exit(bad ? 1 : 0);
