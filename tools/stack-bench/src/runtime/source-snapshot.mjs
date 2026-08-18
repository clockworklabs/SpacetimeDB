import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { basename, join } from 'node:path';

import { hashDirectory } from '../evidence/provenance.mjs';

// Runtime state, dependencies, evidence, and harness-owned control files are
// deliberately not part of the model-authored source snapshot.
const PRESERVED_DIRS = new Set(['node_modules', '.git', 'stack-bench']);
const TRANSIENT_DIRS = new Set(['dist', '.vite', 'coverage']);
const ROOT_HARNESS_FILES = new Set([
  '.lint-port', '.sandbox-settings.json', '.stack-bench-backend',
  'BUG_REPORT.md', 'check-hooks.sh',
]);

const parts = value => value.split(/[\\/]/).filter(Boolean);

function rootHarnessFile(rel) {
  const names = parts(rel);
  if (names.length !== 1) return false;
  return ROOT_HARNESS_FILES.has(names[0]) || /^\.(?:prompt|session)-/.test(names[0]);
}

function directoryDisposition(rel) {
  const name = basename(rel);
  if (PRESERVED_DIRS.has(name)) return 'preserve';
  if (TRANSIENT_DIRS.has(name)) return 'transient';
  return 'source';
}

function copySourceTree(from, to, rel = '') {
  if (!existsSync(from)) return;
  mkdirSync(to, { recursive: true });
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    const childRel = rel ? join(rel, entry.name) : entry.name;
    if (entry.isDirectory() && directoryDisposition(childRel) !== 'source') continue;
    if (!entry.isDirectory() && rootHarnessFile(childRel)) continue;
    const source = join(from, entry.name);
    const target = join(to, entry.name);
    if (entry.isDirectory()) copySourceTree(source, target, childRel);
    else cpSync(source, target, { force: true, dereference: false });
  }
}

// Remove model-authored paths absent from the snapshot by walking them in
// place. Active directory watchers and nested dependency folders survive.
function removeAbsent(path, rel) {
  const stat = lstatSync(path);
  if (!stat.isDirectory()) {
    if (!rootHarnessFile(rel)) rmSync(path, { force: true });
    return;
  }
  const disposition = directoryDisposition(rel);
  if (disposition === 'preserve') return;
  if (disposition === 'transient') {
    rmSync(path, { recursive: true, force: true });
    return;
  }
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    removeAbsent(join(path, entry.name), join(rel, entry.name));
  }
  if (readdirSync(path).length === 0) rmSync(path, { force: true });
}

function syncSourceTree(snapshot, appDir, rel = '') {
  mkdirSync(appDir, { recursive: true });
  const snapshotNames = new Set(readdirSync(snapshot));

  for (const entry of readdirSync(appDir, { withFileTypes: true })) {
    const childRel = rel ? join(rel, entry.name) : entry.name;
    if (entry.isDirectory() && directoryDisposition(childRel) === 'preserve') continue;
    if (!entry.isDirectory() && rootHarnessFile(childRel)) continue;
    if (entry.isDirectory() && directoryDisposition(childRel) === 'transient') {
      rmSync(join(appDir, entry.name), { recursive: true, force: true });
      continue;
    }
    if (!snapshotNames.has(entry.name)) removeAbsent(join(appDir, entry.name), childRel);
  }

  for (const entry of readdirSync(snapshot, { withFileTypes: true })) {
    const childRel = rel ? join(rel, entry.name) : entry.name;
    const source = join(snapshot, entry.name);
    const target = join(appDir, entry.name);
    if (entry.isDirectory()) {
      if (existsSync(target) && !lstatSync(target).isDirectory()) rmSync(target, { force: true });
      syncSourceTree(source, target, childRel);
      continue;
    }
    if (existsSync(target) && lstatSync(target).isDirectory()) {
      removeAbsent(target, childRel);
      if (existsSync(target)) throw new Error(`cannot restore source file over preserved directory: ${childRel}`);
    }
    cpSync(source, target, { force: true, dereference: false });
  }
}

export function snapshotAppSource(appDir, to) {
  rmSync(to, { recursive: true, force: true });
  copySourceTree(appDir, to);
}

export function hashAppSource(appDir) {
  return hashDirectory(appDir, { exclude: (rel, entry) => entry.isDirectory()
    ? directoryDisposition(rel) !== 'source' : rootHarnessFile(rel) });
}

export function restoreAppSource(from, appDir) {
  if (!existsSync(from)) throw new Error(`source snapshot does not exist: ${from}`);
  syncSourceTree(from, appDir);
}

export function seedAppSource(from, appDir) {
  if (!existsSync(from)) throw new Error(`source seed does not exist: ${from}`);
  copySourceTree(from, appDir);
}
