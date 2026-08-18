#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync,
  statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

import { sha256 } from '../evidence/provenance.mjs';
import { validateReleaseManifest, validateSpdxImageSbom } from './release-manifest.mjs';

const DIGEST_REFERENCE = /^[a-z0-9](?:[a-z0-9._:/-]*[a-z0-9])?@sha256:([a-f0-9]{64})$/;

function exactImage(reference) {
  const match = typeof reference === 'string' && !reference.includes('://')
    && reference.match(DIGEST_REFERENCE);
  if (!match) throw new Error('image must be a normalized registry reference at an exact @sha256: digest');
  return { reference, digest: match[1] };
}

function contained(root, relative) {
  if (typeof relative !== 'string' || !relative || relative.startsWith('/')
    || /^[A-Za-z]:[\\/]/.test(relative) || relative.includes('\\')
    || relative.split('/').includes('..')) throw new Error(`invalid bundle path: ${relative}`);
  const target = resolve(root, relative);
  if (!target.startsWith(`${root}${sep}`)) throw new Error(`bundle path escapes root: ${relative}`);
  return target;
}

function run(executable, args, { timeout = 600_000 } = {}) {
  const outcome = spawnSync(executable, args, { encoding: 'utf8', windowsHide: true, timeout,
    maxBuffer: 8 * 1024 * 1024 });
  if (outcome.error) throw outcome.error;
  if (outcome.status !== 0) throw new Error((outcome.stderr || outcome.stdout
    || `${executable} exited ${outcome.status}`).trim().slice(0, 4096));
}

export function generateSpdxImageSbom({ reference, outputPath, platform = 'linux/amd64',
  docker = 'docker', runCommand = run }) {
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

export function materializeReleaseManifest(specification, root) {
  if (!specification || typeof specification !== 'object' || !Array.isArray(specification.files)) {
    throw new Error('release specification must contain a files array');
  }
  const bundleRoot = realpathSync(root);
  const files = specification.files.map((file, index) => {
    if (!file || typeof file !== 'object' || Array.isArray(file)
      || Object.keys(file).some(key => !['path', 'role'].includes(key))) {
      throw new Error(`release specification files[${index}] accepts only path and role`);
    }
    const target = contained(bundleRoot, file.path);
    let actual;
    try { actual = realpathSync(target); }
    catch { throw new Error(`release file is missing: ${file.path}`); }
    if (!actual.startsWith(`${bundleRoot}${sep}`)) throw new Error(`release file escapes through a symlink: ${file.path}`);
    const stat = statSync(actual);
    if (!stat.isFile()) throw new Error(`release file is not regular: ${file.path}`);
    const content = readFileSync(actual);
    return { path: file.path, role: file.role, sha256: sha256(content), bytes: stat.size };
  });
  return validateReleaseManifest({ ...specification, files });
}

function options(args, allowed) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    const value = args[index + 1];
    if (!allowed.includes(flag)) throw new Error(`unknown option ${flag}`);
    if (Object.hasOwn(parsed, flag)) throw new Error(`duplicate option ${flag}`);
    if (!value || value.startsWith('--')) throw new Error(`missing value for ${flag}`);
    parsed[flag] = value;
  }
  for (const flag of allowed) if (!parsed[flag]) throw new Error(`missing ${flag}`);
  return parsed;
}

function usage() {
  return 'Usage:\n'
    + '  node release-bundle.mjs sbom <image@sha256:digest> --output <file>\n'
    + '  node release-bundle.mjs assemble <release-spec.json> --root <bundle-dir> --output <release.json>';
}

function main() {
  const [command, subject, ...args] = process.argv.slice(2);
  if (command === 'sbom' && subject) {
    const parsed = options(args, ['--output']);
    console.log(JSON.stringify(generateSpdxImageSbom({ reference: subject,
      outputPath: parsed['--output'] }), null, 2));
    return;
  }
  if (command === 'assemble' && subject) {
    const parsed = options(args, ['--root', '--output']);
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
  catch (error) {
    console.error(error.message === usage() ? error.message : `release-bundle: ${error.message}`);
    process.exitCode = 2;
  }
}
