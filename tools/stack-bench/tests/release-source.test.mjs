import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { releaseSourceIdentity } from '../release-source.mjs';

function repository() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-'));
  const files = ['.dockerignore', 'licenses/BSL.txt', 'package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml',
    'crates/bindings-typescript/package.json', 'skills/typescript-server/SKILL.md',
    'tools/stack-bench/bench.mjs'];
  for (const path of files) {
    const absolute = join(root, ...path.split('/'));
    mkdirSync(dirname(absolute), { recursive: true });
    writeFileSync(absolute, `${path}\n`);
  }
  mkdirSync(join(root, 'tools', 'stack-bench'), { recursive: true });
  writeFileSync(join(root, 'tools', 'stack-bench', 'JOURNAL.local.md'), 'local only\n');
  return { root, files };
}

function git(files, { changed = '' } = {}) {
  return (_root, args) => {
    if (args[0] === 'rev-parse') return `${'a'.repeat(40)}\n`;
    if (args[0] === 'status') return changed;
    if (args[0] === 'ls-files') return `${files.join('\n')}\n`;
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
    writeFileSync(join(root, 'tools', 'stack-bench', 'JOURNAL.local.md'), 'different local notes\n');
    assert.deepEqual(releaseSourceIdentity(root, { runGit: git(files) }), before);
    writeFileSync(join(root, 'tools', 'stack-bench', 'bench.mjs'), 'changed tracked input\n');
    assert.notEqual(releaseSourceIdentity(root, { runGit: git(files) }).sha256, before.sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release source identity refuses a changed or untracked release input', () => {
  const { root, files } = repository();
  try {
    assert.throws(() => releaseSourceIdentity(root, { runGit: git(files,
      { changed: ' M tools/stack-bench/bench.mjs\n?? tools/stack-bench/local.mjs\n' }) }),
    /release source paths are not clean/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
