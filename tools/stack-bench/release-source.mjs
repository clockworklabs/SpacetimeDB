#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { hashFiles } from './provenance.mjs';

const RELEASE_SOURCE_PATHS = Object.freeze([
  'tools/stack-bench',
  'crates/bindings-typescript',
  'licenses/BSL.txt',
  'package.json',
  'pnpm-lock.yaml',
  'pnpm-workspace.yaml',
  '.dockerignore',
  '.gitattributes',
]);

function defaultGit(root, args) {
  return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8', windowsHide: true,
    maxBuffer: 16 * 1024 * 1024 });
}

export function releaseSourceIdentity(root = process.cwd(), { runGit = defaultGit } = {}) {
  const repository = realpathSync(root);
  const revision = runGit(repository, ['rev-parse', 'HEAD']).trim();
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(revision)) throw new Error('HEAD is not an exact commit id');
  const changed = runGit(repository, ['status', '--porcelain=v1', '--untracked-files=all', '--',
    ...RELEASE_SOURCE_PATHS]).trim();
  if (changed) throw new Error(`release source paths are not clean:\n${changed}`);
  const names = runGit(repository, ['ls-files', '--cached', '--', ...RELEASE_SOURCE_PATHS])
    .split(/\r?\n/).filter(Boolean);
  if (names.length === 0) throw new Error('release source set is empty');
  const identity = hashFiles(names.map(name => resolve(repository, name)), { base: repository });
  return { revision, sha256: identity.sha256, files: identity.files.length,
    paths: [...RELEASE_SOURCE_PATHS] };
}

function main() {
  if (process.argv.length > 3 || (process.argv[2] && process.argv[2] !== '--json')) {
    console.error('Usage: node release-source.mjs [--json]');
    process.exit(2);
  }
  const identity = releaseSourceIdentity();
  if (process.argv[2] === '--json') console.log(JSON.stringify(identity, null, 2));
  else {
    process.stdout.write(`SOURCE_REVISION=${identity.revision}\n`);
    process.stdout.write(`SOURCE_SHA256=${identity.sha256}\n`);
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
