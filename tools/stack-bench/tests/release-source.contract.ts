import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
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

test('Docker source validation accepts CRLF text but rejects real changes and preserves -text attributes', () => {
  const { root } = repository();
  const command = (...args: string[]) => execFileSync('git', ['-C', root, ...args],
    { encoding: 'utf8', windowsHide: true, stdio: 'pipe' }).trim();
  try {
    const dockerfile = readFileSync(join(STACK_BENCH_ROOT, 'appliance', 'Controller.Dockerfile'), 'utf8');
    assert.match(dockerfile, /GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=safe\.directory GIT_CONFIG_VALUE_0=\/checkout/);
    assert.match(dockerfile, /GIT_CONFIG_KEY_1=core\.autocrlf GIT_CONFIG_VALUE_1=input/);
    const binary = 'tools/stack-bench/literal.bin';
    const text = 'crates/bindings-typescript/package.json';
    writeFileSync(join(root, '.gitattributes'), '/tools/stack-bench/** text eol=lf\n*.bin -text\n');
    writeFileSync(join(root, binary), 'literal\nbytes\n');
    writeFileSync(join(root, text), `${text}\r\n`);
    command('init');
    command('config', 'core.autocrlf', 'true');
    command('config', 'user.name', 'Stack Bench test');
    command('config', 'user.email', 'stack-bench@example.invalid');
    command('add', '.');
    const commit = command('commit-tree', command('write-tree'), '-m', 'source fixture');
    command('update-ref', 'HEAD', commit);
    const before = releaseSourceIdentity(root);
    command('config', 'core.autocrlf', 'false');
    // A bind mount has different file metadata; force Git to check the content.
    utimesSync(join(root, text), new Date(), new Date(Date.now() + 10_000));
    assert.throws(() => releaseSourceIdentity(root), /release source paths are not clean/);
    const runGit: GitRunner = (cwd, args, options = {}) => execFileSync('git', ['-C', cwd, ...args], {
      cwd, input: options.input, encoding: options.binary ? null : 'utf8', windowsHide: true,
      env: { ...process.env, GIT_OPTIONAL_LOCKS: '0', GIT_CONFIG_COUNT: '2',
        GIT_CONFIG_KEY_0: 'safe.directory', GIT_CONFIG_VALUE_0: root,
        GIT_CONFIG_KEY_1: 'core.autocrlf', GIT_CONFIG_VALUE_1: 'input' },
    });
    assert.deepEqual(releaseSourceIdentity(root, { runGit }), before);
    writeFileSync(join(root, text), 'actual source edit\r\n');
    assert.throws(() => releaseSourceIdentity(root, { runGit }), /release source paths are not clean/);
    writeFileSync(join(root, text), `${text}\r\n`);
    writeFileSync(join(root, binary), 'literal\r\nbytes\r\n');
    assert.throws(() => releaseSourceIdentity(root, { runGit }), /release source paths are not clean/);
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
