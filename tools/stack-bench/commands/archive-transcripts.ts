#!/usr/bin/env node

import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { operationalOutputRoot } from '../src/runtime/operational-paths.mjs';

interface ArchiveArgs {
  results: string;
  out: string;
  app?: string;
  label?: string;
}

export interface ArchiveResult {
  copied: number;
  missing: number;
  outputDirectory: string;
}

function optionValue(argv: string[], index: number): string {
  const value = argv[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`${argv[index]} requires a value`);
  return value;
}

export function parseArchiveArgs(argv: string[], env: NodeJS.ProcessEnv = process.env): ArchiveArgs {
  const operationalRoot = operationalOutputRoot(STACK_BENCH_ROOT, env);
  const values: { results?: string; out?: string; app?: string; label?: string } = {};
  const seen = new Set<string>();
  for (let index = 2; index < argv.length; index += 2) {
    const flag = argv[index];
    if (!flag || !['--results', '--out', '--app', '--label'].includes(flag)) {
      throw new Error(`unknown option ${String(flag)}`);
    }
    if (seen.has(flag)) throw new Error(`duplicate option ${flag}`);
    seen.add(flag);
    const value = optionValue(argv, index);
    if (flag === '--results') values.results = value;
    else if (flag === '--out') values.out = value;
    else if (flag === '--app') values.app = value;
    else values.label = value;
  }
  const app = values.app ? resolve(values.app) : undefined;
  return {
    results: resolve(values.results ?? join(STACK_BENCH_ROOT, 'results')),
    out: resolve(values.out ?? join(operationalRoot, 'transcripts')),
    ...(app ? { app, label: values.label ?? 'run' } : {}),
  };
}

function encodedStoreName(directory: string): string {
  return resolve(directory).replace(/[\\/:]/g, '-').toLowerCase();
}

function transcriptStoreFor(appDirectory: string, storeRoot: string): string | null {
  if (!existsSync(storeRoot)) return null;
  const expected = encodedStoreName(appDirectory);
  const match = readdirSync(storeRoot).find(directory => {
    const normalized = directory.toLowerCase();
    return normalized === expected || normalized === expected.replace(/^-+/, '');
  });
  return match ? join(storeRoot, match) : null;
}

function collectTranscripts(directory: string, output: string[] = []): string[] {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) collectTranscripts(path, output);
    else if (entry.isFile() && entry.name.endsWith('.jsonl')) output.push(path);
  }
  return output;
}

function archiveJobs(args: ArchiveArgs): Array<{ label: string; appDirectory: string }> {
  if (args.app) return [{ label: args.label ?? 'run', appDirectory: args.app }];
  if (!existsSync(args.results)) throw new Error(`no results directory at ${args.results}`);
  return readdirSync(args.results, { withFileTypes: true })
    .filter(entry => entry.isDirectory())
    .map(entry => ({ label: entry.name,
      appDirectory: join(args.results, entry.name, 'app') }))
    .filter(job => existsSync(job.appDirectory));
}

export function archiveTranscripts(args: ArchiveArgs,
  storeRoot = join(homedir(), '.claude', 'projects')): ArchiveResult {
  mkdirSync(args.out, { recursive: true });
  let copied = 0;
  let missing = 0;
  for (const job of archiveJobs(args)) {
    const store = transcriptStoreFor(job.appDirectory, storeRoot);
    if (!store) {
      console.log(`  ${job.label}: NO TRANSCRIPT in the CLI store — already pruned, or never run`);
      missing += 1;
      continue;
    }
    const destination = join(args.out, job.label);
    mkdirSync(destination, { recursive: true });
    for (const source of collectTranscripts(store)) {
      const relativeName = source.slice(store.length + 1).replace(/[\\/]/g, '__');
      const target = join(destination, relativeName);
      const sourceSize = statSync(source).size;
      if (existsSync(target) && statSync(target).size >= sourceSize) continue;
      copyFileSync(source, target);
      copied += 1;
      console.log(`  ${job.label}/${relativeName}  (${(sourceSize / 1024 / 1024).toFixed(1)} MB)`);
    }
  }
  console.log(`\n${copied} transcript(s) archived to ${args.out}`);
  if (missing) console.log(`${missing} run(s) had no transcript left to archive.`);
  return { copied, missing, outputDirectory: args.out };
}

function main(): void {
  try {
    archiveTranscripts(parseArchiveArgs(process.argv));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
