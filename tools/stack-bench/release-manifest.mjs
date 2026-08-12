#!/usr/bin/env node

import { readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

import { sha256 } from './provenance.mjs';

export const RELEASE_MANIFEST_SCHEMA_VERSION = 1;
const STATE = new Set(['candidate', 'qualified']);
const IMAGE_ROLES = new Set(['controller', 'build-sandbox', 'postgres', 'mongodb']);
const REQUIRED_IMAGE_ROLES = Object.freeze([...IMAGE_ROLES]);
const FILE_ROLES = new Set(['compose', 'dependency', 'operator-guide', 'sbom', 'signature',
  'secrets-template', 'support-policy']);
const REQUIRED_FILE_ROLES = Object.freeze(['compose', 'dependency', 'operator-guide',
  'secrets-template', 'support-policy']);
const HASH = /^[a-f0-9]{64}$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const ID = /^[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*$/;
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
    'sourceSha256', 'supportedRunner', 'images', 'files', 'outboundDestinations', 'secrets']), source);
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
    strict(image, new Set(['id', 'role', 'reference', 'digest', 'platform', 'sbomPath',
      'signaturePath']), at);
    if (typeof image.id !== 'string' || !ID.test(image.id)) throw new Error(`${at}.id is invalid`);
    if (!IMAGE_ROLES.has(image.role)) throw new Error(`${at}.role is invalid`);
    exactHash(image.digest, `${at}.digest`);
    if (typeof image.reference !== 'string'
      || !image.reference.endsWith(`@sha256:${image.digest}`)) {
      throw new Error(`${at}.reference must end in its exact digest`);
    }
    if (image.platform !== 'linux/amd64') throw new Error(`${at}.platform must be linux/amd64`);
    relativePath(image.sbomPath, `${at}.sbomPath`);
    relativePath(image.signaturePath, `${at}.signaturePath`);
  });
  unique(value.images, item => item.id, `${source}.images`);
  unique(value.images, item => item.role, `${source}.images`);
  unique(value.images, item => item.sbomPath, `${source}.images SBOM paths`);
  unique(value.images, item => item.signaturePath, `${source}.images signature paths`);
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
    if (!value.files.some(file => file.path === image.signaturePath && file.role === 'signature')) {
      throw new Error(`${source}: ${image.id} signature is absent from files`);
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

export function verifyReleaseBundle(manifest, root) {
  const validated = validateReleaseManifest(manifest);
  if (validated.state === 'qualified') {
    throw new Error('qualified release verification is unavailable until cryptographic signature verification is implemented');
  }
  const bundleRoot = realpathSync(root);
  const results = validated.files.map(file => {
    const path = contained(bundleRoot, file.path);
    let actualPath;
    try { actualPath = realpathSync(path); }
    catch { return { path: file.path, ok: false, reason: 'missing' }; }
    if (!actualPath.startsWith(`${bundleRoot}${sep}`)) return { path: file.path, ok: false, reason: 'symlink escape' };
    const stat = statSync(actualPath);
    if (!stat.isFile()) return { path: file.path, ok: false, reason: 'not a regular file' };
    const actualHash = sha256(readFileSync(actualPath));
    if (stat.size !== file.bytes) return { path: file.path, ok: false, reason: 'size mismatch' };
    if (actualHash !== file.sha256) return { path: file.path, ok: false, reason: 'hash mismatch' };
    return { path: file.path, ok: true };
  });
  return { ok: results.every(result => result.ok), verificationLevel: 'candidate-file-integrity', release: {
    id: validated.id, version: validated.version, state: validated.state,
  }, results };
}

function main() {
  const [command, manifestPath, rootFlag, rootValue] = process.argv.slice(2);
  if (command !== 'verify' || !manifestPath || rootFlag !== '--root' || !rootValue) {
    console.error('Usage: node release-manifest.mjs verify <release.json> --root <bundle-dir>');
    process.exit(2);
  }
  const result = verifyReleaseBundle(JSON.parse(readFileSync(manifestPath, 'utf8')), rootValue);
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
