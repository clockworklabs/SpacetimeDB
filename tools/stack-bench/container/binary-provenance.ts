#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { binarySourceIdentity, SOURCE_IDENTITY_SCHEME }
  from '../src/releases/release-source.js';

export const RUST_BUILDER_IMAGE =
  'rust:1.93-slim-bookworm@sha256:8f8609d448e821fbc0e44241bc5ca4ce49663cc6306ff1a17f655a0e2a7cd084';
export const BINARY_NAMES = Object.freeze(['spacetimedb-cli', 'spacetimedb-standalone']);
const PROVENANCE_NAME = 'spacetimedb-binaries.json';

interface BinarySourceIdentity {
  identityScheme: typeof SOURCE_IDENTITY_SCHEME;
  revision: string;
  sha256: string;
  files: number;
}

interface BinaryRecord {
  sha256: string;
  size: number;
}

interface BinaryProvenance {
  schemaVersion: 2;
  platform: 'linux/amd64';
  builderImage: string;
  source: BinarySourceIdentity;
  binaries: Record<string, BinaryRecord>;
}

function sha256File(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function binaryPath(stackBenchRoot: string, name: string): string {
  return join(stackBenchRoot, 'container', 'bin', name);
}

function provenancePath(stackBenchRoot: string): string {
  return join(stackBenchRoot, 'container', PROVENANCE_NAME);
}

function assertSha256(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !/^[a-f0-9]{64}$/.test(value)) {
    throw new Error(`${label} must be a SHA-256 digest`);
  }
}

function inspectBinary(path: string, name: string): BinaryRecord {
  if (!existsSync(path)) {
    throw new Error(`${name} is absent; run tools/stack-bench/container/build-linux-cli.sh`);
  }
  const stat = statSync(path);
  if (!stat.isFile() || stat.size < 4) throw new Error(`${name} is not a non-empty file`);
  const magic = readFileSync(path).subarray(0, 4);
  if (!magic.equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
    throw new Error(`${name} is not a Linux ELF binary`);
  }
  return { sha256: sha256File(path), size: stat.size };
}

export function createBinaryProvenance(stackBenchRoot: string,
  source: BinarySourceIdentity): BinaryProvenance {
  assertSha256(source?.sha256, 'binary source identity');
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(source?.revision ?? '')) {
    throw new Error('binary source revision must be an exact commit id');
  }
  const binaries: Record<string, BinaryRecord> = {};
  for (const name of BINARY_NAMES) binaries[name] = inspectBinary(binaryPath(stackBenchRoot, name), name);
  return {
    schemaVersion: 2,
    platform: 'linux/amd64',
    builderImage: RUST_BUILDER_IMAGE,
    source: { identityScheme: source.identityScheme,
      revision: source.revision, sha256: source.sha256, files: source.files },
    binaries,
  };
}

export function assertBinarySourceUnchanged(before: BinarySourceIdentity,
  after: BinarySourceIdentity): void {
  if (before?.identityScheme !== after?.identityScheme
    || before?.revision !== after?.revision || before?.sha256 !== after?.sha256) {
    throw new Error('binary source changed during the build');
  }
}

function readProvenance(stackBenchRoot: string): BinaryProvenance {
  const path = provenancePath(stackBenchRoot);
  if (!existsSync(path)) {
    throw new Error(`${PROVENANCE_NAME} is absent; run tools/stack-bench/container/build-linux-cli.sh`);
  }
  let manifest: BinaryProvenance & { status?: string };
  try { manifest = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    throw new Error(`${PROVENANCE_NAME} is not valid JSON: ${error instanceof Error
      ? error.message : String(error)}`);
  }
  if (manifest.status === 'unbuilt') {
    throw new Error(`${PROVENANCE_NAME} has no verified binaries; run tools/stack-bench/container/build-linux-cli.sh`);
  }
  return manifest;
}

export function verifyBinaryProvenance(stackBenchRoot: string,
  { sourceSha256 }: { sourceSha256?: string } = {}): BinaryProvenance {
  assertSha256(sourceSha256, 'expected binary source identity');
  const manifest = readProvenance(stackBenchRoot);
  if (manifest.schemaVersion !== 2) throw new Error('unsupported binary provenance schema');
  if (manifest.platform !== 'linux/amd64') throw new Error('binary provenance platform must be linux/amd64');
  if (manifest.builderImage !== RUST_BUILDER_IMAGE) {
    throw new Error('binary provenance does not use the pinned Rust builder image');
  }
  assertSha256(manifest.source?.sha256, 'recorded binary source identity');
  if (manifest.source.identityScheme !== SOURCE_IDENTITY_SCHEME) {
    throw new Error('recorded binary source identity scheme is unsupported');
  }
  if (manifest.source.sha256 !== sourceSha256) {
    throw new Error('SpacetimeDB binaries do not match the selected release source');
  }
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(manifest.source?.revision ?? '')) {
    throw new Error('recorded binary source revision is invalid');
  }
  for (const name of BINARY_NAMES) {
    const expected = manifest.binaries?.[name];
    assertSha256(expected?.sha256, `${name} recorded checksum`);
    if (!Number.isSafeInteger(expected.size) || expected.size < 4) {
      throw new Error(`${name} recorded size is invalid`);
    }
    const actual = inspectBinary(binaryPath(stackBenchRoot, name), name);
    if (actual.size !== expected.size) throw new Error(`${name} size does not match provenance`);
    if (actual.sha256 !== expected.sha256) throw new Error(`${name} checksum does not match provenance`);
  }
  return manifest;
}

function option(args: string[], name: string): string {
  const index = args.indexOf(name);
  const value = index === -1 ? undefined : args[index + 1];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function main(): void {
  const [command, ...args] = process.argv.slice(2);
  if (command === 'source') {
    const repo = resolve(option(args, '--repo'));
    console.log(JSON.stringify(binarySourceIdentity(repo), null, 2));
    return;
  }
  if (command === 'record') {
    const repo = resolve(option(args, '--repo'));
    const stackBenchRoot = join(repo, 'tools', 'stack-bench');
    const source = JSON.parse(readFileSync(resolve(option(args, '--source-file')), 'utf8'));
    const current = binarySourceIdentity(repo);
    assertBinarySourceUnchanged(source, current);
    const manifest = createBinaryProvenance(stackBenchRoot, source);
    const path = provenancePath(stackBenchRoot);
    const temporary = `${path}.tmp`;
    writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
    renameSync(temporary, path);
    console.log(`recorded ${path}`);
    return;
  }
  if (command === 'verify') {
    const stackBenchRoot = resolve(option(args, '--root'));
    verifyBinaryProvenance(stackBenchRoot, { sourceSha256: option(args, '--source-sha256') });
    console.log('verified SpacetimeDB CLI and standalone binary provenance');
    return;
  }
  throw new Error('Usage: binary-provenance source --repo PATH | record --repo PATH --source-file PATH | verify --root PATH --source-sha256 SHA256');
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) {
    console.error(`binary provenance failed: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
