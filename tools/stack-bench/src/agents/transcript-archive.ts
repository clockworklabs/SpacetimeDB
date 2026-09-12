import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

import { STACK_BENCH_ROOT } from '../package-root.js';
import { stackBenchResultsRoot } from '../runtime/operational-paths.js';
import { codexTranscriptDirectory } from './codex-protocol.js';

function transcriptStoreFor(appDirectory: string, storeRoot: string): string | null {
  if (!existsSync(storeRoot)) return null;
  const expected = resolve(appDirectory).replace(/[\\/:]/g, '-').toLowerCase();
  const match = readdirSync(storeRoot).find(directory => {
    const normalized = directory.toLowerCase();
    return normalized === expected || normalized === expected.replace(/^-+/, '');
  });
  return match ? join(storeRoot, match) : null;
}

function collectTranscripts(directory: string, recursive: boolean): string[] {
  return readdirSync(directory, { recursive, withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.jsonl'))
    .map(entry => join(entry.parentPath, entry.name));
}

export function transcriptDirectories(appDirectory: string,
  storeRoot = join(homedir(), '.claude', 'projects')): string[] {
  return [transcriptStoreFor(appDirectory, storeRoot), codexTranscriptDirectory(appDirectory)]
    .filter((directory): directory is string => directory !== null && existsSync(directory));
}

export function archiveTranscripts(appDirectory: string, label: string,
  outputDirectory = join(stackBenchResultsRoot(STACK_BENCH_ROOT), 'transcripts'),
  storeRoot = join(homedir(), '.claude', 'projects')) {
  mkdirSync(outputDirectory, { recursive: true });
  const stores = transcriptDirectories(appDirectory, storeRoot);
  if (!stores.length) {
    console.log(`  ${label}: NO TRANSCRIPT in the CLI store — already pruned, or never run`);
    return { copied: 0, missing: 1, outputDirectory };
  }

  const destination = join(outputDirectory, label);
  mkdirSync(destination, { recursive: true });
  let copied = 0;
  for (const store of stores) {
    const codex = store === codexTranscriptDirectory(appDirectory);
    for (const source of collectTranscripts(store, !codex)) {
      if (codex && !source.endsWith('.events.jsonl')) continue;
      const relativeName = source.slice(store.length + 1).replace(/[\\/]/g, '__');
      const target = join(destination, relativeName);
      const sourceSize = statSync(source).size;
      if (existsSync(target) && statSync(target).size >= sourceSize) continue;
      copyFileSync(source, target);
      copied += 1;
      console.log(`  ${label}/${relativeName}  (${(sourceSize / 1024 / 1024).toFixed(1)} MB)`);
    }
  }
  console.log(`\n${copied} transcript(s) archived to ${outputDirectory}`);
  return { copied, missing: 0, outputDirectory };
}
