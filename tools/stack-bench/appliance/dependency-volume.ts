#!/usr/bin/env node

import { createHash } from 'node:crypto';
import {
  chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync,
  renameSync, rmSync, writeFileSync,
} from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const MARKER = '.stack-bench-release-deps.json';

export interface DependencyManifestFile {
  path: string;
  size: number;
  mode: number;
  sha256: string;
}

export interface DependencyManifest {
  schemaVersion: 1;
  files: DependencyManifestFile[];
}

interface DependencyVerification {
  manifestSha256: string;
  files: number;
}

interface DependencyInitialization extends DependencyVerification {
  initialized: boolean;
}

function sha256Bytes(bytes: string | NodeJS.ArrayBufferView): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function normalizedRelative(root: string, path: string): string {
  const value = relative(root, path).split(sep).join('/');
  if (!value || value.startsWith('../') || value === '..') throw new Error(`path escapes dependency root: ${path}`);
  return value;
}

function walk(root: string, current = root): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(current, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`dependency tree cannot contain symlinks: ${normalizedRelative(root, path)}`);
    if (entry.isDirectory()) files.push(...walk(root, path));
    else if (entry.isFile()) files.push(path);
    else throw new Error(`dependency tree contains unsupported entry: ${normalizedRelative(root, path)}`);
  }
  return files;
}

export function createDependencyManifest(root: string): DependencyManifest {
  const absolute = resolve(root);
  if (!existsSync(absolute) || !lstatSync(absolute).isDirectory()) {
    throw new Error(`dependency source is not a directory: ${absolute}`);
  }
  const files = walk(absolute).map(path => {
    const bytes = readFileSync(path);
    return { path: normalizedRelative(absolute, path), size: bytes.length,
      mode: lstatSync(path).mode & 0o777, sha256: sha256Bytes(bytes) };
  });
  if (!files.length) throw new Error('dependency source is empty');
  return { schemaVersion: 1, files };
}

export function manifestSha256(manifest: DependencyManifest): string {
  return sha256Bytes(`${JSON.stringify(manifest)}\n`);
}

export function verifyDependencyTree(root: string, manifest: DependencyManifest,
  { allowMarker = false }: { allowMarker?: boolean } = {}): DependencyVerification {
  if (!manifest || manifest.schemaVersion !== 1 || !Array.isArray(manifest.files) || !manifest.files.length) {
    throw new Error('dependency manifest is invalid');
  }
  const absolute = resolve(root);
  const actual = createDependencyManifest(absolute);
  if (allowMarker) actual.files = actual.files.filter(file => file.path !== MARKER);
  if (JSON.stringify(actual.files) !== JSON.stringify(manifest.files)) {
    throw new Error(`dependency tree does not match manifest ${manifestSha256(manifest)}`);
  }
  return { manifestSha256: manifestSha256(manifest), files: manifest.files.length };
}

export function initializeDependencyVolume({ source, target, manifest }:
  { source: string; target: string; manifest: DependencyManifest }): DependencyInitialization {
  const sourceRoot = resolve(source);
  const targetRoot = resolve(target);
  const verified = verifyDependencyTree(sourceRoot, manifest);
  mkdirSync(targetRoot, { recursive: true, mode: 0o755 });
  const markerPath = join(targetRoot, MARKER);
  const existing = readdirSync(targetRoot);
  if (existing.length) {
    if (!existsSync(markerPath)) throw new Error('dependency volume is non-empty but has no release marker');
    const marker = JSON.parse(readFileSync(markerPath, 'utf8'));
    if (marker.schemaVersion !== 1 || marker.manifestSha256 !== verified.manifestSha256) {
      throw new Error('dependency volume belongs to a different release');
    }
    verifyDependencyTree(targetRoot, manifest, { allowMarker: true });
    return { ...verified, initialized: false };
  }

  const staging = join(targetRoot, `.staging-${process.pid}`);
  mkdirSync(staging, { mode: 0o700 });
  try {
    for (const file of manifest.files) {
      const from = join(sourceRoot, ...file.path.split('/'));
      const to = join(staging, ...file.path.split('/'));
      mkdirSync(dirname(to), { recursive: true });
      copyFileSync(from, to);
      chmodSync(to, file.mode);
    }
    for (const entry of readdirSync(staging)) renameSync(join(staging, entry), join(targetRoot, entry));
    rmSync(staging, { recursive: true, force: true });
    writeFileSync(markerPath, `${JSON.stringify({ schemaVersion: 1,
      manifestSha256: verified.manifestSha256 })}\n`, { flag: 'wx', mode: 0o444 });
    verifyDependencyTree(targetRoot, manifest, { allowMarker: true });
    return { ...verified, initialized: true };
  } catch (error) {
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
}

function main(argv: string[]): void {
  const { values, positionals } = parseArgs({ args: argv.slice(2), allowPositionals: true,
    options: {
      source: { type: 'string', default: '/opt/stack-bench-embedded-deps' },
      target: { type: 'string', default: '/opt/stack-bench-release-deps' },
      manifest: { type: 'string', default: '/opt/stack-bench/dependency-manifest.json' },
      out: { type: 'string' },
    } });
  const [command] = positionals;
  const source = values.source;
  const target = values.target;
  const manifestPath = values.manifest;
  if (command === 'manifest') {
    const output = values.out;
    if (!output) throw new Error('manifest requires --out');
    writeFileSync(resolve(output), `${JSON.stringify(createDependencyManifest(source), null, 2)}\n`, { flag: 'wx' });
    return;
  }
  const manifest: DependencyManifest = JSON.parse(readFileSync(resolve(manifestPath), 'utf8'));
  if (command === 'init') {
    process.stdout.write(`${JSON.stringify(initializeDependencyVolume({ source, target, manifest }))}\n`);
    return;
  }
  if (command === 'verify') {
    process.stdout.write(`${JSON.stringify(verifyDependencyTree(target, manifest, { allowMarker: true }))}\n`);
    return;
  }
  throw new Error('usage: dependency-volume manifest|init|verify [options]');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try { main(process.argv); }
  catch (error) {
    console.error(`dependency-volume: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(2);
  }
}
