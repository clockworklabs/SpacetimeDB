#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { isDeepStrictEqual } from 'node:util';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

import { sha256 } from './provenance.mjs';

export const RELEASE_MANIFEST_SCHEMA_VERSION = 2;
const STATE = new Set(['candidate', 'qualified']);
const IMAGE_ROLES = new Set(['controller', 'build-sandbox', 'postgres', 'mongodb']);
const REQUIRED_IMAGE_ROLES = Object.freeze([...IMAGE_ROLES]);
const FILE_ROLES = new Set(['compose', 'dependency', 'operator-guide', 'public-key', 'sbom',
  'secrets-template', 'support-policy']);
const REQUIRED_FILE_ROLES = Object.freeze(['compose', 'dependency', 'operator-guide',
  'secrets-template', 'support-policy']);
const HASH = /^[a-f0-9]{64}$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const ID = /^[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*$/;
const IMAGE_REFERENCE = /^[a-z0-9](?:[a-z0-9._:/-]*[a-z0-9])?@sha256:([a-f0-9]{64})$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function strict(value, fields, at) {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
}

function relativePath(value, at) {
  if (typeof value !== 'string' || !value || value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value)
    || value.split(/[\\/]/).includes('..') || value.includes('\\')) {
    throw new Error(`${at} must be a normalized relative POSIX path`);
  }
  return value;
}

function exactHash(value, at) {
  if (typeof value !== 'string' || !HASH.test(value)) throw new Error(`${at} must be a SHA-256`);
  return value;
}

function unique(items, key, at) {
  const seen = new Set();
  for (const [index, item] of items.entries()) {
    const value = key(item);
    if (seen.has(value)) throw new Error(`${at}[${index}] duplicates ${value}`);
    seen.add(value);
  }
}

export function validateReleaseManifest(value, { source = 'release manifest' } = {}) {
  strict(value, new Set(['schemaVersion', 'id', 'version', 'state', 'sourceRevision',
    'sourceSha256', 'supportedRunner', 'images', 'files', 'outboundDestinations', 'secrets',
    'signing']), source);
  if (value.schemaVersion !== RELEASE_MANIFEST_SCHEMA_VERSION) throw new Error(`${source}.schemaVersion is unsupported`);
  if (typeof value.id !== 'string' || !ID.test(value.id)) throw new Error(`${source}.id is invalid`);
  if (typeof value.version !== 'string' || !VERSION.test(value.version)) throw new Error(`${source}.version is invalid`);
  if (!STATE.has(value.state)) throw new Error(`${source}.state must be candidate or qualified`);
  if (typeof value.sourceRevision !== 'string' || !/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(value.sourceRevision)) {
    throw new Error(`${source}.sourceRevision must be a 40- or 64-character commit id`);
  }
  exactHash(value.sourceSha256, `${source}.sourceSha256`);

  strict(value.supportedRunner, new Set(['os', 'architecture', 'stateRoot', 'networkMode',
    'dockerSocket']), `${source}.supportedRunner`);
  if (value.supportedRunner.os !== 'linux' || value.supportedRunner.architecture !== 'amd64'
    || value.supportedRunner.networkMode !== 'host' || value.supportedRunner.dockerSocket !== true) {
    throw new Error(`${source}.supportedRunner must declare the v1 dedicated linux/amd64 host-network Docker-socket topology`);
  }
  if (typeof value.supportedRunner.stateRoot !== 'string'
    || !value.supportedRunner.stateRoot.startsWith('/')) throw new Error(`${source}.supportedRunner.stateRoot must be absolute`);

  if (!Array.isArray(value.images)) throw new Error(`${source}.images must be an array`);
  value.images.forEach((image, index) => {
    const at = `${source}.images[${index}]`;
    strict(image, new Set(['id', 'role', 'reference', 'digest', 'platform', 'sbomPath']), at);
    if (typeof image.id !== 'string' || !ID.test(image.id)) throw new Error(`${at}.id is invalid`);
    if (!IMAGE_ROLES.has(image.role)) throw new Error(`${at}.role is invalid`);
    exactHash(image.digest, `${at}.digest`);
    const reference = typeof image.reference === 'string' && !image.reference.includes('://')
      && image.reference.match(IMAGE_REFERENCE);
    if (!reference || reference[1] !== image.digest) {
      throw new Error(`${at}.reference must be a normalized registry reference at its exact digest`);
    }
    if (image.platform !== 'linux/amd64') throw new Error(`${at}.platform must be linux/amd64`);
    relativePath(image.sbomPath, `${at}.sbomPath`);
  });
  unique(value.images, item => item.id, `${source}.images`);
  unique(value.images, item => item.role, `${source}.images`);
  unique(value.images, item => item.sbomPath, `${source}.images SBOM paths`);
  for (const role of REQUIRED_IMAGE_ROLES) {
    if (!value.images.some(image => image.role === role)) throw new Error(`${source}.images is missing ${role}`);
  }

  if (!Array.isArray(value.files)) throw new Error(`${source}.files must be an array`);
  value.files.forEach((file, index) => {
    const at = `${source}.files[${index}]`;
    strict(file, new Set(['path', 'role', 'sha256', 'bytes']), at);
    relativePath(file.path, `${at}.path`);
    if (!FILE_ROLES.has(file.role)) throw new Error(`${at}.role is invalid`);
    exactHash(file.sha256, `${at}.sha256`);
    if (!Number.isSafeInteger(file.bytes) || file.bytes < 0) throw new Error(`${at}.bytes is invalid`);
  });
  unique(value.files, item => item.path, `${source}.files`);
  for (const role of REQUIRED_FILE_ROLES) {
    if (!value.files.some(file => file.role === role)) throw new Error(`${source}.files is missing ${role}`);
  }
  for (const image of value.images) {
    if (!value.files.some(file => file.path === image.sbomPath && file.role === 'sbom')) {
      throw new Error(`${source}: ${image.id} SBOM is absent from files`);
    }
  }

  if (value.state === 'candidate') {
    if (value.signing !== null) throw new Error(`${source}.signing must be null for a candidate`);
    if (value.files.some(file => file.role === 'public-key')) {
      throw new Error(`${source}.files cannot claim a signing key for a candidate`);
    }
  } else {
    strict(value.signing, new Set(['scheme', 'publicKeyPath', 'manifestBundlePath']),
      `${source}.signing`);
    if (value.signing.scheme !== 'cosign-public-key-v1') {
      throw new Error(`${source}.signing.scheme must be cosign-public-key-v1`);
    }
    relativePath(value.signing.publicKeyPath, `${source}.signing.publicKeyPath`);
    relativePath(value.signing.manifestBundlePath, `${source}.signing.manifestBundlePath`);
    if (!value.files.some(file => file.path === value.signing.publicKeyPath
      && file.role === 'public-key')) {
      throw new Error(`${source}.signing public key is absent from files`);
    }
    if (value.files.some(file => file.path === value.signing.manifestBundlePath)) {
      throw new Error(`${source}.signing manifest bundle cannot checksum itself`);
    }
  }

  if (!Array.isArray(value.outboundDestinations)) throw new Error(`${source}.outboundDestinations must be an array`);
  value.outboundDestinations.forEach((entry, index) => {
    const at = `${source}.outboundDestinations[${index}]`;
    strict(entry, new Set(['owner', 'url']), at);
    if (typeof entry.owner !== 'string' || !ID.test(entry.owner)) throw new Error(`${at}.owner is invalid`);
    try { if (new URL(entry.url).protocol !== 'https:') throw new Error(); }
    catch { throw new Error(`${at}.url must be HTTPS`); }
  });
  unique(value.outboundDestinations, item => `${item.owner}:${item.url}`, `${source}.outboundDestinations`);

  if (!Array.isArray(value.secrets)) throw new Error(`${source}.secrets must be an array`);
  value.secrets.forEach((secret, index) => {
    const at = `${source}.secrets[${index}]`;
    strict(secret, new Set(['id', 'adapter', 'composeTarget', 'required']), at);
    if (typeof secret.id !== 'string' || !ID.test(secret.id)) throw new Error(`${at}.id is invalid`);
    if (typeof secret.adapter !== 'string' || !ID.test(secret.adapter)) throw new Error(`${at}.adapter is invalid`);
    if (typeof secret.composeTarget !== 'string' || !secret.composeTarget.startsWith('/run/secrets/')
      || secret.composeTarget.split('/').includes('..')) {
      throw new Error(`${at}.composeTarget must be under /run/secrets`);
    }
    if (typeof secret.required !== 'boolean') throw new Error(`${at}.required must be boolean`);
  });
  unique(value.secrets, item => item.id, `${source}.secrets`);
  return structuredClone(value);
}

function contained(root, relative) {
  const target = resolve(root, relative);
  if (target === root || !target.startsWith(`${root}${sep}`)) throw new Error(`release file escapes bundle root: ${relative}`);
  return target;
}

function regularContainedFile(root, relative) {
  const path = contained(root, relative);
  let actualPath;
  try { actualPath = realpathSync(path); }
  catch { return { path, ok: false, reason: 'missing' }; }
  if (!actualPath.startsWith(`${root}${sep}`)) return { path, ok: false, reason: 'symlink escape' };
  if (!statSync(actualPath).isFile()) return { path, ok: false, reason: 'not a regular file' };
  return { path: actualPath, ok: true };
}

export function validateSpdxImageSbom(value, image, { source = image.sbomPath } = {}) {
  if (!object(value)) throw new Error(`${source} must be an object`);
  if (value.spdxVersion !== 'SPDX-2.3' || value.dataLicense !== 'CC0-1.0'
    || value.SPDXID !== 'SPDXRef-DOCUMENT') throw new Error(`${source} is not an SPDX 2.3 JSON document`);
  if (!object(value.creationInfo) || !Array.isArray(value.creationInfo.creators)
    || value.creationInfo.creators.length === 0) throw new Error(`${source}.creationInfo is incomplete`);
  if (!Array.isArray(value.packages) || value.packages.length === 0) {
    throw new Error(`${source}.packages must be non-empty`);
  }
  const locators = value.packages.flatMap(entry => Array.isArray(entry.externalRefs)
    ? entry.externalRefs.map(reference => reference.referenceLocator) : []);
  const digestLocator = new RegExp(`^pkg:oci/[^?]+@sha256:${image.digest}(?:\\?|$)`);
  if (!locators.some(locator => typeof locator === 'string' && digestLocator.test(locator))) {
    throw new Error(`${source} does not bind image digest sha256:${image.digest}`);
  }
  return value;
}

function defaultRun(executable, args) {
  const outcome = spawnSync(executable, args, { encoding: 'utf8', windowsHide: true,
    timeout: 120_000, maxBuffer: 8 * 1024 * 1024 });
  if (outcome.error) return { ok: false, detail: outcome.error.message };
  if (outcome.status !== 0) return { ok: false,
    detail: (outcome.stderr || outcome.stdout || `exit ${outcome.status}`).trim().slice(0, 4096) };
  return { ok: true };
}

function normalizeRunResult(value) {
  if (value && typeof value === 'object' && typeof value.ok === 'boolean') {
    return { ok: value.ok, ...(typeof value.detail === 'string' && value.detail
      ? { detail: value.detail.slice(0, 4096) } : {}) };
  }
  return { ok: false, detail: 'verification command returned an invalid result' };
}

export function verifyReleaseBundle(manifest, root, options = {}) {
  const validated = validateReleaseManifest(manifest);
  const bundleRoot = realpathSync(root);
  const results = validated.files.map(file => {
    const resolved = regularContainedFile(bundleRoot, file.path);
    if (!resolved.ok) return { path: file.path, ok: false, reason: resolved.reason };
    const stat = statSync(resolved.path);
    const actualHash = sha256(readFileSync(resolved.path));
    if (stat.size !== file.bytes) return { path: file.path, ok: false, reason: 'size mismatch' };
    if (actualHash !== file.sha256) return { path: file.path, ok: false, reason: 'hash mismatch' };
    return { path: file.path, ok: true };
  });
  for (const image of validated.images) {
    if (!results.find(result => result.path === image.sbomPath)?.ok) continue;
    try {
      const path = regularContainedFile(bundleRoot, image.sbomPath).path;
      validateSpdxImageSbom(JSON.parse(readFileSync(path, 'utf8')), image);
      results.push({ path: image.sbomPath, check: 'spdx-image-binding', ok: true });
    } catch (error) {
      results.push({ path: image.sbomPath, check: 'spdx-image-binding', ok: false,
        reason: error.message });
    }
  }
  const release = { id: validated.id, version: validated.version, state: validated.state };
  if (validated.state === 'candidate') return { ok: results.every(result => result.ok),
    verificationLevel: 'candidate-file-integrity', release, results };

  if (!results.every(result => result.ok)) return { ok: false,
    verificationLevel: 'qualified-cryptographic', release, results,
    cryptographicVerification: { ok: false, reason: 'skipped because bundle integrity failed' } };

  if (!options.manifestPath) throw new Error('qualified verification requires the exact signed manifest path');
  if (!options.trustedKeyPath) throw new Error('qualified verification requires an external trusted public key');
  const manifestPath = realpathSync(options.manifestPath);
  if (!manifestPath.startsWith(`${bundleRoot}${sep}`) || !statSync(manifestPath).isFile()) {
    throw new Error('signed manifest must be a regular file inside the bundle root');
  }
  const diskManifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  if (!isDeepStrictEqual(diskManifest, validated)) {
    throw new Error('the supplied manifest object differs from the signed manifest file');
  }
  const trustedKey = realpathSync(options.trustedKeyPath);
  if (!statSync(trustedKey).isFile()) throw new Error('trusted public key must be a regular file');
  if (trustedKey.startsWith(`${bundleRoot}${sep}`)) {
    throw new Error('trusted public key must be supplied outside the release bundle');
  }
  const bundledKey = regularContainedFile(bundleRoot, validated.signing.publicKeyPath);
  if (!bundledKey.ok) throw new Error(`bundled public key is ${bundledKey.reason}`);
  if (!readFileSync(trustedKey).equals(readFileSync(bundledKey.path))) {
    throw new Error('bundled public key differs from the external trusted public key');
  }
  const bundle = regularContainedFile(bundleRoot, validated.signing.manifestBundlePath);
  if (!bundle.ok) throw new Error(`manifest signature bundle is ${bundle.reason}`);

  const run = options.runCommand ?? defaultRun;
  const executable = options.cosignPath ?? 'cosign';
  const checks = [{ subject: 'release-manifest', ...normalizeRunResult(run(executable,
    ['verify-blob', '--key', trustedKey, '--bundle', bundle.path, manifestPath])) }];
  for (const image of validated.images) {
    checks.push({ subject: image.reference, ...normalizeRunResult(run(executable,
      ['verify', '--key', trustedKey, '--output', 'json', image.reference])) });
  }
  const cryptographicVerification = { ok: checks.every(check => check.ok), checks };
  return { ok: cryptographicVerification.ok, verificationLevel: 'qualified-cryptographic',
    release, results, cryptographicVerification };
}

function parseCli(argv) {
  const [command, manifestPath, ...rest] = argv;
  if (command !== 'verify' || !manifestPath) throw new Error('usage');
  const options = {};
  for (let index = 0; index < rest.length; index += 2) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (!value || !['--root', '--trusted-key'].includes(flag)) throw new Error('usage');
    if (flag === '--root') options.root = value;
    else options.trustedKeyPath = value;
  }
  if (!options.root) throw new Error('usage');
  return { manifestPath, ...options };
}

function main() {
  let args;
  try { args = parseCli(process.argv.slice(2)); }
  catch {
    console.error('Usage: node release-manifest.mjs verify <release.json> --root <bundle-dir> [--trusted-key <cosign.pub>]');
    process.exit(2);
  }
  const result = verifyReleaseBundle(JSON.parse(readFileSync(args.manifestPath, 'utf8')),
    args.root, { manifestPath: args.manifestPath, trustedKeyPath: args.trustedKeyPath });
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) {
    console.error(`release verification: ${error.message}`);
    process.exitCode = 2;
  }
}
