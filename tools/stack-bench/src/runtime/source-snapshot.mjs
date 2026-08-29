import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { basename, join } from 'node:path';

import { hashDirectory } from '../evidence/provenance.js';

// Runtime state, dependencies, evidence, and harness-owned control files are
// deliberately not part of the model-authored source snapshot.
const PRESERVED_DIRS = new Set(['node_modules', '.git', 'stack-bench']);
const TRANSIENT_DIRS = new Set([
  'dist', '.vite', 'coverage',
  // Package and browser tooling can create large trees with dangling links.
  // They are runtime caches, not model-authored application source.
  '.apt', '.cache', '.debroot', '.libs', '.pw-browsers', '.pwcache',
]);
const TRANSIENT_PATHS = new Set(['client/src/module_bindings']);
const ROOT_HARNESS_FILES = new Set([
  '.sandbox-settings.json', '.stack-bench-backend',
  'BUG_REPORT.md', 'bug-report-quality.json',
]);
const RUNTIME_LOG_FILES = new Set(['client.log', 'server.log', 'vite.log']);

const parts = value => value.split(/[\\/]/).filter(Boolean);

function rootHarnessFile(rel) {
  const names = parts(rel);
  if (names.length !== 1) return false;
  return ROOT_HARNESS_FILES.has(names[0]) || /^\.(?:prompt|session)-/.test(names[0]);
}

function preservedRuntimeFile(rel) {
  return rootHarnessFile(rel) || RUNTIME_LOG_FILES.has(basename(rel));
}

function directoryDisposition(rel) {
  const normalized = rel.replaceAll('\\', '/');
  if (TRANSIENT_PATHS.has(normalized)) return 'transient';
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
    if (!entry.isDirectory() && preservedRuntimeFile(childRel)) continue;
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
    if (!preservedRuntimeFile(rel)) rmSync(path, { force: true });
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
    if (!entry.isDirectory() && preservedRuntimeFile(childRel)) continue;
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
    ? directoryDisposition(rel) !== 'source' : preservedRuntimeFile(rel) });
}

export function assertAppSourceIdentity(appDir, expectedSha256, context = 'application source') {
  const actual = hashAppSource(appDir);
  if (actual.sha256 !== expectedSha256) {
    throw new Error(`${context} hash ${actual.sha256} does not match ${expectedSha256}`);
  }
  return actual;
}

// Validate only the files that belong to the source identity. Dependency,
// build-output, and harness directories can contain links created by their
// own tools and are excluded from the source snapshot and hash.
export function assertPlainAppSourceTree(appDir) {
  if (!existsSync(appDir)) return;
  if (!lstatSync(appDir).isDirectory()) throw new Error('application source root is not a plain directory');
  const walk = (directory, rel = '') => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const childRel = rel ? join(rel, entry.name) : entry.name;
      if (entry.isDirectory() && directoryDisposition(childRel) !== 'source') continue;
      if (!entry.isDirectory() && preservedRuntimeFile(childRel)) continue;
      if (entry.isDirectory()) walk(join(directory, entry.name), childRel);
      else if (!entry.isFile()) {
        throw new Error(`application source contains unsupported filesystem entry ${childRel}`);
      }
    }
  };
  walk(appDir);
}

export function restoreAppSource(from, appDir) {
  if (!existsSync(from)) throw new Error(`source snapshot does not exist: ${from}`);
  syncSourceTree(from, appDir);
}

export function seedAppSource(from, appDir) {
  if (!existsSync(from)) throw new Error(`source seed does not exist: ${from}`);
  copySourceTree(from, appDir);
}
