#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { hashFiles } from '../evidence/provenance.mjs';
import { REPOSITORY_ROOT } from '../project-paths.mjs';

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

export function releaseSourceRoot() {
  return realpathSync(REPOSITORY_ROOT);
}

function defaultGit(root, args) {
  return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', windowsHide: true,
    maxBuffer: 16 * 1024 * 1024 });
}

function trackedNames(repository, paths, runGit) {
  return runGit(repository, ['ls-files', '--cached', '--', ...paths])
    .split(/\r?\n/).filter(Boolean);
}

export function binarySourceIdentity(root = process.cwd(), { runGit = defaultGit,
  requireClean = false } = {}) {
  const repository = realpathSync(root);
  const revision = runGit(repository, ['rev-parse', 'HEAD']).trim();
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(revision)) {
    throw new Error('HEAD is not an exact commit id');
  }
  if (requireClean) {
    const changed = runGit(repository, ['status', '--porcelain=v1', '--untracked-files=all', '--',
      ...BINARY_SOURCE_PATHS, BINARY_PROVENANCE_PATH]).trim();
    if (changed) throw new Error(`binary source paths are not clean:\n${changed}`);
  }
  const names = trackedNames(repository, BINARY_SOURCE_PATHS, runGit)
    .filter(name => name !== BINARY_PROVENANCE_PATH);
  if (names.length === 0) throw new Error('binary source set is empty');
  const identity = hashFiles(names.map(name => resolve(repository, name)), { base: repository });
  return { revision, sha256: identity.sha256, files: identity.files.length,
    paths: [...BINARY_SOURCE_PATHS] };
}

export function releaseSourceIdentity(root = process.cwd(), { runGit = defaultGit } = {}) {
  const repository = realpathSync(root);
  const revision = runGit(repository, ['rev-parse', 'HEAD']).trim();
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(revision)) throw new Error('HEAD is not an exact commit id');
  const changed = runGit(repository, ['status', '--porcelain=v1', '--untracked-files=all', '--',
    ...RELEASE_SOURCE_PATHS]).trim();
  if (changed) throw new Error(`release source paths are not clean:\n${changed}`);
  const names = trackedNames(repository, RELEASE_SOURCE_PATHS, runGit);
  if (names.length === 0) throw new Error('release source set is empty');
  const identity = hashFiles(names.map(name => resolve(repository, name)), { base: repository });
  const binarySource = binarySourceIdentity(repository, { runGit, requireClean: true });
  return { revision, sha256: identity.sha256, files: identity.files.length,
    paths: [...RELEASE_SOURCE_PATHS], binarySourceSha256: binarySource.sha256,
    binarySourceFiles: binarySource.files };
}

function main() {
  if (process.argv.length > 3 || (process.argv[2] && process.argv[2] !== '--json')) {
    console.error('Usage: node src/releases/release-source.mjs [--json]');
    process.exit(2);
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
