#!/usr/bin/env node
// Verify that a mutation file can apply to its reference app without grading.

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

interface CliArgs {
  app: string;
  mutations: string;
  quiet: boolean;
}

interface MutationEdit {
  file: string;
  find: string;
}

interface MutationDefinition {
  id?: unknown;
}

interface MutationSpec {
  anchoredTo?: unknown;
  mutations?: MutationDefinition[];
}

interface MutationValidationIssue {
  kind: string;
  mutation?: string;
}

interface MutationAnalysis {
  mutationFileEdits: (mutation: MutationDefinition) => MutationEdit[];
  resolveMutationFile: (appDirectory: string, relativePath: string) => string;
  validateMutationDefinitions: (mutations: MutationDefinition[] | undefined) => {
    issues: MutationValidationIssue[];
  };
}

function parseArgs(argv: string[]): CliArgs {
  let app: string | undefined;
  let mutations: string | undefined;
  let quiet = false;
  for (let index = 2; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--app') app = argv[++index];
    else if (value === '--mutations') mutations = argv[++index];
    else if (value === '--quiet') quiet = true;
    else {
      console.error(`Unknown arg ${String(value)}`);
      process.exit(2);
    }
  }
  if (!app || !mutations) {
    console.error('Usage: node check-mutations.mjs --app <dir> --mutations <file.json>');
    process.exit(2);
  }
  return { app, mutations, quiet };
}

async function loadMutationAnalysis(): Promise<MutationAnalysis> {
  const sourceUrl = pathToFileURL(join(
    STACK_BENCH_ROOT,
    'src',
    'evidence',
    'mutation-analysis.mjs',
  )).href;
  const module: unknown = await import(sourceUrl);
  if (typeof module !== 'object' || module === null
    || !('mutationFileEdits' in module) || typeof module.mutationFileEdits !== 'function'
    || !('resolveMutationFile' in module) || typeof module.resolveMutationFile !== 'function'
    || !('validateMutationDefinitions' in module)
    || typeof module.validateMutationDefinitions !== 'function') {
    throw new Error('mutation analysis is unavailable');
  }
  return module as MutationAnalysis;
}

const args = parseArgs(process.argv);
const parsed: unknown = JSON.parse(readFileSync(args.mutations, 'utf8'));
if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
  throw new Error('mutation manifest must be an object');
}
const spec = parsed as MutationSpec;
const { mutationFileEdits, resolveMutationFile, validateMutationDefinitions }
  = await loadMutationAnalysis();
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
