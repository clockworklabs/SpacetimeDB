import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

import { STACK_BENCH_ROOT } from '../package-root.js';
import { stackBenchResultsRoot } from '../runtime/operational-paths.js';

function transcriptStoreFor(appDirectory: string, storeRoot: string): string | null {
  if (!existsSync(storeRoot)) return null;
  const expected = resolve(appDirectory).replace(/[\\/:]/g, '-').toLowerCase();
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

export function archiveTranscripts(appDirectory: string, label: string,
  outputDirectory = join(stackBenchResultsRoot(STACK_BENCH_ROOT), 'transcripts'),
  storeRoot = join(homedir(), '.claude', 'projects')) {
  mkdirSync(outputDirectory, { recursive: true });
  const store = transcriptStoreFor(appDirectory, storeRoot);
  if (!store) {
    console.log(`  ${label}: NO TRANSCRIPT in the CLI store — already pruned, or never run`);
    return { copied: 0, missing: 1, outputDirectory };
  }

  const destination = join(outputDirectory, label);
  mkdirSync(destination, { recursive: true });
  let copied = 0;
  for (const source of collectTranscripts(store)) {
    const relativeName = source.slice(store.length + 1).replace(/[\\/]/g, '__');
    const target = join(destination, relativeName);
    const sourceSize = statSync(source).size;
    if (existsSync(target) && statSync(target).size >= sourceSize) continue;
    copyFileSync(source, target);
    copied += 1;
    console.log(`  ${label}/${relativeName}  (${(sourceSize / 1024 / 1024).toFixed(1)} MB)`);
  }
  console.log(`\n${copied} transcript(s) archived to ${outputDirectory}`);
  return { copied, missing: 0, outputDirectory };
}
