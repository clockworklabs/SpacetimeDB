#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync,
  statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

import { sha256 } from '../evidence/provenance.js';
import { validateReleaseManifest, validateSpdxImageSbom } from './release-manifest.js';
import type { ReleaseFileRole } from './release-manifest.js';

const DIGEST_REFERENCE = /^[a-z0-9](?:[a-z0-9._:/-]*[a-z0-9])?@sha256:([a-f0-9]{64})$/;

interface ImageReference { reference: string; digest: string }
interface RunOptions { timeout?: number }
export type BundleRunCommand = (executable: string, args: string[], options?: RunOptions) => void;
interface GenerateSbomOptions {
  reference: string;
  outputPath: string;
  platform?: string;
  docker?: string;
  runCommand?: BundleRunCommand;
}
interface ReleaseSpecificationFile { path: string; role: ReleaseFileRole }
type UnknownRecord = Record<string, unknown>;

function exactImage(reference: string): ImageReference {
  const match = !reference.includes('://') && reference.match(DIGEST_REFERENCE);
  if (!match) throw new Error('image must be a normalized registry reference at an exact @sha256: digest');
  return { reference, digest: match[1]! };
}

function contained(root: string, relative: unknown): string {
  if (typeof relative !== 'string' || !relative || relative.startsWith('/')
    || /^[A-Za-z]:[\\/]/.test(relative) || relative.includes('\\')
    || relative.split('/').includes('..')) throw new Error(`invalid bundle path: ${relative}`);
  const target = resolve(root, relative);
  if (!target.startsWith(`${root}${sep}`)) throw new Error(`bundle path escapes root: ${relative}`);
  return target;
}

function run(executable: string, args: string[], { timeout = 600_000 }: RunOptions = {}): void {
  const outcome = spawnSync(executable, args, { encoding: 'utf8', windowsHide: true, timeout,
    maxBuffer: 8 * 1024 * 1024 });
  if (outcome.error) throw outcome.error;
  if (outcome.status !== 0) throw new Error((outcome.stderr || outcome.stdout
    || `${executable} exited ${outcome.status}`).trim().slice(0, 4096));
}

export function generateSpdxImageSbom({ reference, outputPath, platform = 'linux/amd64',
  docker = 'docker', runCommand = run }: GenerateSbomOptions) {
  const image = exactImage(reference);
  if (platform !== 'linux/amd64') throw new Error('v1 release SBOM platform must be linux/amd64');
  const output = resolve(outputPath);
  if (existsSync(output)) throw new Error(`refusing to overwrite existing SBOM: ${output}`);
  mkdirSync(dirname(output), { recursive: true });
  const temporary = mkdtempSync(join(dirname(output), '.stack-bench-sbom-'));
  const staged = join(temporary, 'image.spdx.json');
  try {
    runCommand(docker, ['scout', 'sbom', '--format', 'spdx', '--platform', platform,
      '--output', staged, `registry://${reference}`]);
    const document = JSON.parse(readFileSync(staged, 'utf8'));
    validateSpdxImageSbom(document, { ...image, sbomPath: output });
    renameSync(staged, output);
    return { path: output, sha256: sha256(readFileSync(output)), bytes: statSync(output).size,
      image: reference, platform };
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

export function materializeReleaseManifest(specification: unknown, root: string) {
  if (!specification || typeof specification !== 'object' || Array.isArray(specification)
    || !Array.isArray((specification as UnknownRecord).files)) {
    throw new Error('release specification must contain a files array');
  }
  const record = specification as UnknownRecord & { files: unknown[] };
  const bundleRoot = realpathSync(root);
  const files = record.files.map((file, index) => {
    if (!file || typeof file !== 'object' || Array.isArray(file)
      || Object.keys(file).some(key => !['path', 'role'].includes(key))) {
      throw new Error(`release specification files[${index}] accepts only path and role`);
    }
    const specificationFile = file as Partial<ReleaseSpecificationFile>;
    if (typeof specificationFile.path !== 'string' || typeof specificationFile.role !== 'string') {
      throw new Error(`release specification files[${index}] accepts only path and role`);
    }
    const target = contained(bundleRoot, specificationFile.path);
    let actual: string;
    try { actual = realpathSync(target); }
    catch { throw new Error(`release file is missing: ${specificationFile.path}`); }
    if (!actual.startsWith(`${bundleRoot}${sep}`)) {
      throw new Error(`release file escapes through a symlink: ${specificationFile.path}`);
    }
    const stat = statSync(actual);
    if (!stat.isFile()) throw new Error(`release file is not regular: ${specificationFile.path}`);
    const content = readFileSync(actual);
    return { path: specificationFile.path, role: specificationFile.role,
      sha256: sha256(content), bytes: stat.size };
  });
  return validateReleaseManifest({ ...record, files });
}

function options<T extends string>(args: string[], allowed: readonly T[]): Record<T, string> {
  const parsed: Partial<Record<T, string>> = {};
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (!flag || !allowed.includes(flag as T)) throw new Error(`unknown option ${flag}`);
    if (Object.hasOwn(parsed, flag)) throw new Error(`duplicate option ${flag}`);
    if (!value || value.startsWith('--')) throw new Error(`missing value for ${flag}`);
    parsed[flag as T] = value;
  }
  for (const flag of allowed) if (!parsed[flag]) throw new Error(`missing ${flag}`);
  return parsed as Record<T, string>;
}

function usage(): string {
  return 'Usage:\n'
    + '  node dist/src/releases/release-bundle.js sbom <image@sha256:digest> --output <file>\n'
    + '  node dist/src/releases/release-bundle.js assemble <release-spec.json> --root <bundle-dir> --output <release.json>';
}

function main(): void {
  const [command, subject, ...args] = process.argv.slice(2);
  if (command === 'sbom' && subject) {
    const parsed = options(args, ['--output'] as const);
    console.log(JSON.stringify(generateSpdxImageSbom({ reference: subject,
      outputPath: parsed['--output'] }), null, 2));
    return;
  }
  if (command === 'assemble' && subject) {
    const parsed = options(args, ['--root', '--output'] as const);
    const root = parsed['--root'];
    const output = resolve(parsed['--output']);
    const manifest = materializeReleaseManifest(JSON.parse(readFileSync(subject, 'utf8')), root);
    const bundleRoot = realpathSync(root);
    if (!output.startsWith(`${bundleRoot}${sep}`)) throw new Error('release manifest output must be inside bundle root');
    if (existsSync(output)) throw new Error(`refusing to overwrite release manifest: ${output}`);
    mkdirSync(dirname(output), { recursive: true });
    writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
    console.log(JSON.stringify({ path: output, id: manifest.id, version: manifest.version,
      state: manifest.state, files: manifest.files.length }, null, 2));
    return;
  }
  throw new Error(usage());
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(message === usage() ? message : `release-bundle: ${message}`);
    process.exitCode = 2;
  }
}
