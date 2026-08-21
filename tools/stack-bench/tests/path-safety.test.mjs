import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync,
  writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { assertNewOrEmptyDirectory } from '../src/runtime/path-safety.mjs';

test('new and empty output directories are accepted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-path-'));
  try {
    const missing = join(root, 'missing');
    assert.equal(assertNewOrEmptyDirectory(missing, 'output'), missing);
    const empty = join(root, 'empty');
    mkdirSync(empty);
    assert.equal(assertNewOrEmptyDirectory(empty, 'output'), empty);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('existing content is refused and left untouched', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-path-'));
  try {
    const target = join(root, 'owned-by-caller');
    mkdirSync(target);
    const sentinel = join(target, 'keep.txt');
    writeFileSync(sentinel, 'keep me');
    assert.throws(() => assertNewOrEmptyDirectory(target, 'output'), /refusing to overwrite/);
    assert.equal(existsSync(sentinel), true);
    assert.equal(readFileSync(sentinel, 'utf8'), 'keep me');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('symlink destinations are refused even when their target is empty', { skip: process.platform === 'win32' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-path-'));
  try {
    const target = join(root, 'target');
    const link = join(root, 'link');
    mkdirSync(target);
    symlinkSync(target, link, 'dir');
    assert.throws(() => assertNewOrEmptyDirectory(link, 'output'), /real directory/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
