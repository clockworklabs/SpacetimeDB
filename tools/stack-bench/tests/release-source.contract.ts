import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { REPOSITORY_ROOT, STACK_BENCH_ROOT } from '../src/package-root.js';
import { releaseSourceIdentity, releaseSourceRoot } from '../src/releases/release-source.js';
import type { GitRunner } from '../src/releases/release-source.js';

test('the release-source CLI anchors itself to the repository instead of the caller cwd', () => {
  assert.equal(releaseSourceRoot(), realpathSync(REPOSITORY_ROOT));
});

function repository(): { root: string; files: string[] } {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-'));
  const files = ['licenses/BSL.txt', 'package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml',
    'crates/bindings-typescript/package.json', 'skills/typescript-server/SKILL.md',
    'tools/stack-bench/commands/bench.ts',
    'tools/stack-bench/container/spacetimedb-binaries.json'];
  for (const path of files) {
    const absolute = join(root, ...path.split('/'));
    mkdirSync(dirname(absolute), { recursive: true });
    writeFileSync(absolute, `${path}\n`);
  }
  mkdirSync(join(root, 'tools', 'stack-bench'), { recursive: true });
  writeFileSync(join(root, 'tools', 'stack-bench', 'JOURNAL.local.md'), 'local only\n');
  return { root, files };
}

function canonicalBytes(root: string, path: string): Buffer {
  return Buffer.from(readFileSync(join(root, ...path.split('/')), 'utf8').replaceAll('\r\n', '\n'));
}

function blobId(bytes: Buffer): string {
  return createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
}

function git(files: string[], { changed = '' }: { changed?: string } = {}): GitRunner {
  return (root, args, options = {}) => {
    if (args[0] === 'rev-parse') return `${'a'.repeat(40)}\n`;
    if (args[0] === 'status') return changed;
    if (args[0] === 'ls-files') return files.map(path => {
      const bytes = canonicalBytes(root, path);
      return `100644 ${blobId(bytes)} 0\t${path}\0`;
    }).join('');
    if (args[0] === 'cat-file') {
      const byId = new Map(files.map(path => {
        const bytes = canonicalBytes(root, path);
        return [blobId(bytes), bytes] as const;
      }));
      const objects = String(options.input ?? '').trim().split('\n').filter(Boolean).map(id => {
        const bytes = byId.get(id);
        if (!bytes) throw new Error(`unknown fake git object ${id}`);
        return Buffer.concat([Buffer.from(`${id} blob ${bytes.length}\n`), bytes, Buffer.from('\n')]);
      });
      return Buffer.concat(objects);
    }
    throw new Error(`unexpected git call ${args.join(' ')}`);
  };
}

test('release source identity hashes only the exact tracked build inputs', () => {
  const { root, files } = repository();
  try {
    const before = releaseSourceIdentity(root, { runGit: git(files) });
    assert.equal(before.revision, 'a'.repeat(40));
    assert.equal(before.files, files.length);
    assert.equal(before.paths.includes('pnpm-lock.yaml'), true);
    assert.equal(before.paths.includes('skills'), true);
    assert.equal(before.paths.includes('.dockerignore'), false);
    assert.equal(before.paths.includes('.gitattributes'), false);
    writeFileSync(join(root, 'tools', 'stack-bench', 'JOURNAL.local.md'), 'different local notes\n');
    assert.deepEqual(releaseSourceIdentity(root, { runGit: git(files) }), before);
    writeFileSync(join(root, 'tools', 'stack-bench', 'container',
      'spacetimedb-binaries.json'), 'changed provenance\n');
    const provenanceChanged = releaseSourceIdentity(root, { runGit: git(files) });
    assert.notEqual(provenanceChanged.sha256, before.sha256);
    assert.equal(provenanceChanged.binarySourceSha256, before.binarySourceSha256);
    writeFileSync(join(root, 'tools', 'stack-bench', 'commands', 'bench.ts'), 'changed tracked input\n');
    assert.notEqual(releaseSourceIdentity(root, { runGit: git(files) }).sha256, before.sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release source identity refuses a changed or untracked release input', () => {
  const { root, files } = repository();
  try {
    assert.throws(() => releaseSourceIdentity(root, { runGit: git(files,
      { changed: ' M tools/stack-bench/commands/bench.ts\n?? tools/stack-bench/local.ts\n' }) }),
    /release source paths are not clean/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('controller build context includes every repository root copied by its Dockerfile', () => {
  const dockerfile = readFileSync(join(STACK_BENCH_ROOT, 'appliance', 'Controller.Dockerfile'), 'utf8');
  const ignore = readFileSync(join(STACK_BENCH_ROOT, 'appliance',
    'Controller.Dockerfile.dockerignore'), 'utf8');
  const roots = [...dockerfile.matchAll(/^COPY (?!-)\s*([^/\s]+)(?:\/|\s)/gm)].map(match => match[1]);
  for (const root of new Set(roots)) {
    assert.match(ignore, new RegExp(`^!${root}(?:\\r?\\n|/)`, 'm'),
      `${root} is copied but excluded from the controller build context`);
  }
});

test('controller build context excludes ignored local Stack Bench state', () => {
  const ignore = readFileSync(join(STACK_BENCH_ROOT, 'appliance',
    'Controller.Dockerfile.dockerignore'), 'utf8');
  const rules = new Set(ignore.split(/\r?\n/));
  const localPaths = [
    'tools/stack-bench/local-notes',
    'tools/stack-bench/media',
    'tools/stack-bench/snapshot-l*',
    'tools/stack-bench/grader/.candidates',
    'tools/stack-bench/grader/.mutation-report.json',
    'tools/stack-bench/tracks/*/overview.html',
  ];
  for (const path of localPaths) {
    assert.equal(rules.has(path), true, `${path} can leak into the controller build context`);
  }
});
