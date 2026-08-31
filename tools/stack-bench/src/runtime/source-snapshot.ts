import { chmodSync, cpSync, existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { basename, join } from 'node:path';

import { hashDirectory } from '../evidence/provenance.js';
import type { HashFilesResult } from '../evidence/provenance.js';

// Snapshots contain model-authored source, not runtime state or repair evidence.
const PRESERVED_DIRS = new Set(['node_modules']);
const ROOT_PRESERVED_DIRS = new Set(['.git', 'stack-bench']);
const TRANSIENT_DIRS = new Set([
  'dist', '.vite', 'coverage',
  '.apt', '.cache', '.debroot', '.libs', '.npm-cache', '.pw-browsers', '.pwcache',
]);
const TRANSIENT_PATHS = new Set(['client/src/module_bindings']);
const ROOT_RUNTIME_FILES = new Set(['BUG_REPORT.md', 'client.log', 'server.log', 'vite.log']);

type DirectoryDisposition = 'preserve' | 'transient' | 'source';

function preservedRuntimeFile(rel: string): boolean {
  return ROOT_RUNTIME_FILES.has(rel.replaceAll('\\', '/'));
}

const transientRuntimeFile = (rel: string): boolean => basename(rel).endsWith('.tsbuildinfo');

function directoryDisposition(rel: string): DirectoryDisposition {
  const normalized = rel.replaceAll('\\', '/');
  if (TRANSIENT_PATHS.has(normalized)) return 'transient';
  const name = basename(rel);
  if (ROOT_PRESERVED_DIRS.has(normalized)) return 'preserve';
  if (PRESERVED_DIRS.has(name)) return 'preserve';
  if (TRANSIENT_DIRS.has(name)) return 'transient';
  return 'source';
}

function copySourceTree(from: string, to: string, rel = '', writable = false): void {
  if (!existsSync(from)) return;
  mkdirSync(to, { recursive: true });
  if (writable) chmodSync(to, lstatSync(to).mode | 0o222);
  for (const entry of readdirSync(from, { withFileTypes: true })) {
    const childRel = rel ? join(rel, entry.name) : entry.name;
    if (directoryDisposition(childRel) !== 'source') continue;
    if (!entry.isDirectory() && (preservedRuntimeFile(childRel) || transientRuntimeFile(childRel))) continue;
    const source = join(from, entry.name);
    const target = join(to, entry.name);
    if (entry.isDirectory()) copySourceTree(source, target, childRel, writable);
    else if (entry.isFile()) {
      cpSync(source, target, { force: true, dereference: false });
      // Materialized source runs as an unprivileged UID in its own workspace.
      if (writable) chmodSync(target, lstatSync(target).mode | 0o222);
    } else throw new Error(`application source contains unsupported filesystem entry ${childRel}`);
  }
}

// Restore source in place without replacing active dependency directories.
function removeAbsent(path: string, rel: string): void {
  const disposition = directoryDisposition(rel);
  if (disposition === 'preserve') return;
  if (disposition === 'transient') {
    rmSync(path, { recursive: true, force: true });
    return;
  }
  const stat = lstatSync(path);
  if (!stat.isDirectory()) {
    if (!preservedRuntimeFile(rel)) rmSync(path, { force: true });
    return;
  }
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    removeAbsent(join(path, entry.name), join(rel, entry.name));
  }
  if (readdirSync(path).length === 0) rmSync(path, { recursive: true, force: true });
}

function syncSourceTree(snapshot: string, appDir: string, rel = ''): void {
  mkdirSync(appDir, { recursive: true });
  chmodSync(appDir, lstatSync(appDir).mode | 0o222);
  const snapshotNames = new Set(readdirSync(snapshot));

  for (const entry of readdirSync(appDir, { withFileTypes: true })) {
    const childRel = rel ? join(rel, entry.name) : entry.name;
    const disposition = directoryDisposition(childRel);
    if (disposition === 'preserve') continue;
    if (!entry.isDirectory() && preservedRuntimeFile(childRel)) continue;
    if (disposition === 'transient') {
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
    if (!entry.isFile()) {
      throw new Error(`source snapshot contains unsupported filesystem entry ${childRel}`);
    }
    if (existsSync(target) && !lstatSync(target).isFile()) {
      if (lstatSync(target).isDirectory()) {
        removeAbsent(target, childRel);
        if (existsSync(target)) {
          throw new Error(`cannot restore source file over preserved directory: ${childRel}`);
        }
      } else rmSync(target, { force: true });
    }
    cpSync(source, target, { force: true, dereference: false });
    chmodSync(target, lstatSync(target).mode | 0o222);
  }
}

export function snapshotAppSource(appDir: string, to: string): void {
  rmSync(to, { recursive: true, force: true });
  copySourceTree(appDir, to);
}

export function hashAppSource(appDir: string): HashFilesResult {
  return hashDirectory(appDir, { exclude: (rel) => directoryDisposition(rel) !== 'source'
    || preservedRuntimeFile(rel) || transientRuntimeFile(rel) });
}

export function assertAppSourceIdentity(appDir: string, expectedSha256: string,
  context = 'application source'): HashFilesResult {
  const actual = hashAppSource(appDir);
  if (actual.sha256 !== expectedSha256) {
    throw new Error(`${context} hash ${actual.sha256} does not match ${expectedSha256}`);
  }
  return actual;
}

// Validate only the files that belong to the source identity. Dependency,
// build-output, and harness directories can contain links created by their
// own tools and are excluded from the source snapshot and hash.
export function assertPlainAppSourceTree(appDir: string): void {
  if (!existsSync(appDir)) return;
  if (!lstatSync(appDir).isDirectory()) throw new Error('application source root is not a plain directory');
  const walk = (directory: string, rel = ''): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const childRel = rel ? join(rel, entry.name) : entry.name;
      if (directoryDisposition(childRel) !== 'source') continue;
      if (!entry.isDirectory() && (preservedRuntimeFile(childRel) || transientRuntimeFile(childRel))) continue;
      if (entry.isDirectory()) walk(join(directory, entry.name), childRel);
      else if (!entry.isFile()) {
        throw new Error(`application source contains unsupported filesystem entry ${childRel}`);
      }
    }
  };
  walk(appDir);
}

export function restoreAppSource(from: string, appDir: string): void {
  if (!existsSync(from)) throw new Error(`source snapshot does not exist: ${from}`);
  syncSourceTree(from, appDir);
}

export function resetAppToSource(from: string, appDir: string): void {
  if (!existsSync(from)) throw new Error(`source snapshot does not exist: ${from}`);
  mkdirSync(appDir, { recursive: true });
  for (const entry of readdirSync(appDir, { withFileTypes: true })) {
    if (entry.name === '.git') continue;
    rmSync(join(appDir, entry.name), { recursive: entry.isDirectory(), force: true });
  }
  copySourceTree(from, appDir, '', true);
}

export function seedAppSource(from: string, appDir: string): void {
  if (!existsSync(from)) throw new Error(`source seed does not exist: ${from}`);
  copySourceTree(from, appDir, '', true);
}
