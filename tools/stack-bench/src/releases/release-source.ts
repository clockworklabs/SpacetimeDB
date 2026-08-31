#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { REPOSITORY_ROOT } from '../package-root.js';

const RELEASE_SOURCE_PATHS = Object.freeze([
  'tools/stack-bench',
  'skills',
  'crates/bindings-typescript',
  'licenses/BSL.txt',
  'package.json',
  'pnpm-lock.yaml',
  'pnpm-workspace.yaml',
]);

export const BINARY_SOURCE_PATHS = Object.freeze([
  '.cargo',
  'Cargo.lock',
  'Cargo.toml',
  'crates',
  'rust-toolchain.toml',
  'skills',
  'templates',
]);

export const BINARY_PROVENANCE_PATH =
  'tools/stack-bench/container/spacetimedb-binaries.json';
export const SOURCE_IDENTITY_SCHEME = 'git-object-content-v1';

interface GitOptions {
  input?: string;
  binary?: boolean;
}

export type GitRunner = (root: string, args: string[], options?: GitOptions) => string | Buffer;

interface SourceIdentity {
  identityScheme: typeof SOURCE_IDENTITY_SCHEME;
  revision: string;
  sha256: string;
  files: number;
  paths: string[];
}

export interface ReleaseSourceIdentity extends SourceIdentity {
  binarySourceSha256: string;
  binarySourceFiles: number;
}

interface TrackedEntry {
  name: string;
  objectId: string;
}

export function releaseSourceRoot(): string {
  return realpathSync(REPOSITORY_ROOT);
}

function defaultGit(root: string, args: string[], options: GitOptions = {}): string | Buffer {
  const common = { cwd: root, input: options.input, windowsHide: true,
    maxBuffer: 512 * 1024 * 1024 };
  return options.binary
    ? execFileSync('git', ['-C', root, ...args], { ...common, encoding: null })
    : execFileSync('git', ['-C', root, ...args], { ...common, encoding: 'utf8' });
}

function gitText(runGit: GitRunner, root: string, args: string[], input?: string): string {
  const result = runGit(root, args, { input });
  return Buffer.isBuffer(result) ? result.toString('utf8') : result;
}

function gitBuffer(runGit: GitRunner, root: string, args: string[], input: string): Buffer {
  const result = runGit(root, args, { input, binary: true });
  return Buffer.isBuffer(result) ? result : Buffer.from(result, 'utf8');
}

function trackedEntries(repository: string, paths: readonly string[],
  runGit: GitRunner): TrackedEntry[] {
  const output = gitText(runGit, repository, ['ls-files', '--stage', '-z', '--', ...paths]);
  const entries = output.split('\0').filter(Boolean).map(row => {
    const separator = row.indexOf('\t');
    if (separator < 0) throw new Error('git returned an invalid tracked source entry');
    const [mode, objectId, stage] = row.slice(0, separator).split(' ');
    const name = row.slice(separator + 1);
    if (!mode || !/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(objectId ?? '') || stage !== '0') {
      throw new Error(`git returned an invalid tracked source entry for ${name}`);
    }
    if (name.includes('\n') || name.includes('\r')) {
      throw new Error(`tracked source path contains a line break: ${JSON.stringify(name)}`);
    }
    return { name, objectId: objectId as string };
  });
  return entries.sort((left, right) => left.name.localeCompare(right.name));
}

function readBatchObjects(output: Buffer, entries: TrackedEntry[]): Buffer[] {
  const objects: Buffer[] = [];
  let offset = 0;
  for (const entry of entries) {
    const headerEnd = output.indexOf(0x0a, offset);
    if (headerEnd < 0) throw new Error(`git returned no object header for ${entry.name}`);
    const header = output.subarray(offset, headerEnd).toString('utf8');
    const match = header.match(/^([a-f0-9]{40}(?:[a-f0-9]{24})?) blob (\d+)$/);
    if (!match || match[1] !== entry.objectId) {
      throw new Error(`git returned an invalid object header for ${entry.name}`);
    }
    const size = Number(match[2]);
    if (!Number.isSafeInteger(size) || size < 0) {
      throw new Error(`git returned an invalid object size for ${entry.name}`);
    }
    const start = headerEnd + 1;
    const end = start + size;
    if (end >= output.length || output[end] !== 0x0a) {
      throw new Error(`git returned a truncated object for ${entry.name}`);
    }
    objects.push(output.subarray(start, end));
    offset = end + 1;
  }
  if (offset !== output.length) throw new Error('git returned unexpected extra object data');
  return objects;
}

function canonicalTrackedIdentity(repository: string, entries: TrackedEntry[],
  runGit: GitRunner): { sha256: string; files: number } {
  const input = `${entries.map(entry => entry.objectId).join('\n')}\n`;
  const objects = readBatchObjects(
    gitBuffer(runGit, repository, ['cat-file', '--batch'], input), entries);
  const hash = createHash('sha256');
  entries.forEach((entry, index) => {
    const bytes = objects[index];
    if (!bytes) throw new Error(`git returned no object for ${entry.name}`);
    hash.update(`${entry.name.length}:${entry.name}:${bytes.length}:`);
    hash.update(bytes);
  });
  return { sha256: hash.digest('hex'), files: entries.length };
}

function exactRevision(repository: string, runGit: GitRunner): string {
  const revision = gitText(runGit, repository, ['rev-parse', 'HEAD']).trim();
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(revision)) {
    throw new Error('HEAD is not an exact commit id');
  }
  return revision;
}

function assertClean(repository: string, paths: readonly string[], runGit: GitRunner,
  label: string): void {
  const changed = gitText(runGit, repository,
    ['status', '--porcelain=v1', '--untracked-files=all', '--', ...paths]).trim();
  if (changed) throw new Error(`${label} paths are not clean:\n${changed}`);
}

export function binarySourceIdentity(root = process.cwd(),
  { runGit = defaultGit }: { runGit?: GitRunner } = {}): SourceIdentity {
  const repository = realpathSync(root);
  const revision = exactRevision(repository, runGit);
  assertClean(repository, [...BINARY_SOURCE_PATHS, BINARY_PROVENANCE_PATH], runGit,
    'binary source');
  const entries = trackedEntries(repository, BINARY_SOURCE_PATHS, runGit)
    .filter(entry => entry.name !== BINARY_PROVENANCE_PATH);
  if (entries.length === 0) throw new Error('binary source set is empty');
  const identity = canonicalTrackedIdentity(repository, entries, runGit);
  return { identityScheme: SOURCE_IDENTITY_SCHEME,
    revision, ...identity, paths: [...BINARY_SOURCE_PATHS] };
}

export function releaseSourceIdentity(root = process.cwd(),
  { runGit = defaultGit }: { runGit?: GitRunner } = {}): ReleaseSourceIdentity {
  const repository = realpathSync(root);
  const revision = exactRevision(repository, runGit);
  assertClean(repository, RELEASE_SOURCE_PATHS, runGit, 'release source');
  const entries = trackedEntries(repository, RELEASE_SOURCE_PATHS, runGit);
  if (entries.length === 0) throw new Error('release source set is empty');
  const identity = canonicalTrackedIdentity(repository, entries, runGit);
  const binarySource = binarySourceIdentity(repository, { runGit });
  return { identityScheme: SOURCE_IDENTITY_SCHEME,
    revision, ...identity, paths: [...RELEASE_SOURCE_PATHS],
    binarySourceSha256: binarySource.sha256, binarySourceFiles: binarySource.files };
}

function main(): void {
  if (process.argv.length > 3 || (process.argv[2] && process.argv[2] !== '--json')) {
    console.error('Usage: release-source [--json]');
    process.exitCode = 2;
    return;
  }
  const identity = releaseSourceIdentity(releaseSourceRoot());
  if (process.argv[2] === '--json') console.log(JSON.stringify(identity, null, 2));
  else {
    process.stdout.write(`SOURCE_REVISION=${identity.revision}\n`);
    process.stdout.write(`SOURCE_SHA256=${identity.sha256}\n`);
    process.stdout.write(`BINARY_SOURCE_SHA256=${identity.binarySourceSha256}\n`);
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
