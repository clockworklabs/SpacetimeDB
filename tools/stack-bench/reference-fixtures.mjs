#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hashDirectory } from './provenance.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REGISTRY = join(ROOT, 'reference-apps', 'registry.json');
const BACKENDS = new Set(['spacetime', 'postgres', 'mongodb']);
const STATUSES = new Set(['blocked', 'candidate', 'active']);
const ORIGIN_KINDS = new Set(['authored', 'historical-import']);
const FIXTURE_KINDS = new Set(['node-api', 'spacetime']);
const FORBIDDEN_DIRECTORIES = new Set(['node_modules', 'dist', 'module_bindings', 'stack-bench']);
const FORBIDDEN_FILES = [/^\.env(?:\..*)?$/i, /\.mutation-backup(?:\..*)?$/i];
const WORKSTATION_PATHS = [/[A-Z]:[\\/](?:Users|Development)[\\/]/i];
const TRUNCATION_MARKER = /…\d+\s+(?:tokens|chars)\s+truncated…/i;

export function loadReferenceRegistry(path = REGISTRY) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function validateReferenceRegistry(registry, { root = ROOT } = {}) {
  const issues = [];
  if (registry?.schemaVersion !== 3) issues.push('schemaVersion must be 3');
  if (!Array.isArray(registry?.fixtures) || registry.fixtures.length === 0) {
    return { ok: false, issues: [...issues, 'fixtures must be a non-empty array'] };
  }
  const ids = new Set();
  const tuples = new Set();
  const referencedManifests = new Map();
  for (const fixture of registry.fixtures) {
    const label = fixture.id ?? '<unnamed>';
    if (typeof fixture.id !== 'string' || !fixture.id || ids.has(fixture.id)) issues.push(`${label}: id is missing or duplicated`);
    ids.add(fixture.id);
    if (!BACKENDS.has(fixture.backend)) issues.push(`${label}: invalid backend`);
    if (typeof fixture.track !== 'string' || !fixture.track) issues.push(`${label}: track is required`);
    if (!Number.isInteger(fixture.level) || fixture.level < 1) issues.push(`${label}: level must be a positive integer`);
    if (!STATUSES.has(fixture.status)) issues.push(`${label}: invalid status`);
    const tuple = `${fixture.track}:${fixture.backend}:${fixture.level}`;
    if (tuples.has(tuple)) issues.push(`${label}: duplicate track/backend/level`);
    tuples.add(tuple);
    if (typeof fixture.targetPath !== 'string' || !fixture.targetPath.startsWith('reference-apps/')) {
      issues.push(`${label}: targetPath must stay under reference-apps/`);
    }
    if (!Array.isArray(fixture.mutationManifests)) issues.push(`${label}: mutationManifests must be an array`);
    for (const manifestPath of fixture.mutationManifests ?? []) {
      if (referencedManifests.has(manifestPath)) issues.push(`${label}: ${manifestPath} is already owned by ${referencedManifests.get(manifestPath)}`);
      referencedManifests.set(manifestPath, label);
      const full = join(root, manifestPath);
      if (!existsSync(full)) issues.push(`${label}: missing mutation manifest ${manifestPath}`);
      else {
        const manifest = JSON.parse(readFileSync(full, 'utf8'));
        if (manifest.backend !== fixture.backend || manifest.track !== fixture.track || Number(manifest.level) !== fixture.level) {
          issues.push(`${label}: ${manifestPath} targets a different backend/track/level`);
        }
        const expectedStatus = fixture.status === 'active' ? 'active'
          : fixture.status === 'candidate' ? 'candidate'
          : 'legacy-unreproducible';
        if (manifest.status !== expectedStatus) issues.push(`${label}: ${manifestPath} must be ${expectedStatus} while fixture is ${fixture.status}`);
        if (fixture.status !== 'blocked' && manifest.fixtureSha256 !== fixture.imported?.sourceSha256) {
          issues.push(`${label}: ${manifestPath} fixtureSha256 must match the imported fixture`);
        }
      }
    }
    if (fixture.status === 'blocked') {
      if (typeof fixture.blockedReason !== 'string' || !fixture.blockedReason) issues.push(`${label}: blockedReason is required`);
      continue;
    }
    if (!ORIGIN_KINDS.has(fixture.origin?.kind)) {
      issues.push(`${label}: origin kind must be authored or historical-import`);
    } else if (fixture.origin.kind === 'historical-import') {
      if (typeof fixture.origin.source !== 'string'
          || !/^[a-f0-9]{64}$/.test(fixture.origin.sourceSha256 ?? '')) {
        issues.push(`${label}: historical origin source and sourceSha256 are required`);
      } else if (!isSafeRelativePath(fixture.origin.source)
          || !fixture.origin.source.replaceAll('\\', '/').startsWith('archive/pre-v1/results/')) {
        issues.push(`${label}: historical origin must stay inside archive/pre-v1/results`);
      }
      if (!Array.isArray(fixture.archivedEvidence) || fixture.archivedEvidence.length === 0) {
        issues.push(`${label}: historical origin requires archivedEvidence`);
      }
      for (const path of fixture.archivedEvidence ?? []) {
        if (!isSafeRelativePath(path)
          || !path.replaceAll('\\', '/').startsWith('archive/pre-v1/results/')) {
          issues.push(`${label}: archived evidence must stay inside archive/pre-v1/results`);
        }
      }
    } else {
      if (typeof fixture.origin.note !== 'string' || !fixture.origin.note.trim()) {
        issues.push(`${label}: authored origin requires a provenance note`);
      }
      if (fixture.archivedEvidence !== undefined
          && (!Array.isArray(fixture.archivedEvidence) || fixture.archivedEvidence.length !== 0)) {
        issues.push(`${label}: authored origin cannot claim archivedEvidence`);
      }
    }
    if (!fixture.imported || fixture.imported.path !== fixture.targetPath ||
        !/^[a-f0-9]{64}$/.test(fixture.imported.sourceSha256 ?? '')) {
      issues.push(`${label}: imported path and sourceSha256 are required and must identify targetPath`);
    } else {
      const inspection = inspectImportedReference(fixture, { root });
      for (const failure of inspection.failures) issues.push(`${label}: ${failure}`);
    }
  }
  const manifestDir = join(root, 'grader', 'mutations');
  if (existsSync(manifestDir)) {
    for (const file of readdirSync(manifestDir).filter(name => name.endsWith('.json'))) {
      const path = `grader/mutations/${file}`;
      if (!referencedManifests.has(path)) issues.push(`unowned mutation manifest: ${path}`);
    }
  }
  return { ok: issues.length === 0, issues };
}

export function inspectImportedReference(fixture, { root = ROOT } = {}) {
  const target = join(root, fixture.imported?.path ?? fixture.targetPath ?? '');
  const failures = [];
  if (!existsSync(target)) return { id: fixture.id, available: false, ok: false, failures: ['imported fixture directory is missing'] };

  const sourceHash = hashDirectory(target);
  if (sourceHash.sha256 !== fixture.imported?.sourceSha256) failures.push('imported fixture hash does not match registry');
  const metadataPath = join(target, 'reference.json');
  let metadata;
  if (!existsSync(metadataPath)) failures.push('reference.json is missing');
  else {
    try { metadata = JSON.parse(readFileSync(metadataPath, 'utf8')); }
    catch { failures.push('reference.json is not valid JSON'); }
  }
  if (metadata) {
    if (metadata.schemaVersion !== 1) failures.push('reference.json schemaVersion must be 1');
    if (!FIXTURE_KINDS.has(metadata.kind)) failures.push('reference.json kind is invalid');
    if (!Array.isArray(metadata.installDirectories) || metadata.installDirectories.length === 0) {
      failures.push('reference.json installDirectories must be non-empty');
    } else {
      for (const directory of metadata.installDirectories) {
        if (!isSafeRelativePath(directory)) failures.push(`unsafe install directory ${directory}`);
        else {
          const packagePath = join(target, directory, 'package.json');
          const lockPath = join(target, directory, 'package-lock.json');
          if (!existsSync(packagePath)) failures.push(`${directory}/package.json is missing`);
          else readJson(packagePath, `${directory}/package.json`, failures);
          if (!existsSync(lockPath)) failures.push(`${directory}/package-lock.json is missing`);
          else {
            const lock = readJson(lockPath, `${directory}/package-lock.json`, failures);
            if (lock && (!Number.isInteger(lock.lockfileVersion) || lock.lockfileVersion < 1)) {
              failures.push(`${directory}/package-lock.json has an invalid lockfileVersion`);
            }
          }
        }
      }
    }
  }

  for (const name of sourceHash.files) {
    const segments = name.split('/');
    if (segments.some(segment => FORBIDDEN_DIRECTORIES.has(segment))) failures.push(`forbidden generated directory in ${name}`);
    if (FORBIDDEN_FILES.some(pattern => pattern.test(basename(name)))) failures.push(`forbidden local file ${name}`);
    const full = join(target, ...segments);
    const bytes = readFileSync(full);
    if (!bytes.includes(0)) {
      const contents = bytes.toString('utf8');
      if (WORKSTATION_PATHS.some(pattern => pattern.test(contents))) failures.push(`workstation absolute path in ${name}`);
      if (TRUNCATION_MARKER.test(contents)) failures.push(`tool-output truncation marker in ${name}`);
    }
  }
  return { id: fixture.id, available: true, ok: failures.length === 0,
    sourceSha256: sourceHash.sha256, sourceFiles: sourceHash.files.length, failures };
}

export function inspectReferenceCandidate(fixture, { root = ROOT } = {}) {
  if (fixture.origin?.kind === 'authored') {
    const imported = inspectImportedReference(fixture, { root });
    return { id: fixture.id, origin: 'authored', available: imported.available,
      ok: imported.ok, sourceSha256: imported.sourceSha256,
      sourceFiles: imported.sourceFiles, archivedEvidence: 0, failures: imported.failures };
  }
  const source = join(root, fixture.origin?.source ?? '');
  const failures = [];
  if (!existsSync(source)) return { id: fixture.id, available: false, ok: false, failures: ['source is unavailable'] };
  const sourceHash = hashDirectory(source);
  if (sourceHash.sha256 !== fixture.origin.sourceSha256) failures.push('source hash does not match registry');
  for (const evidencePath of fixture.archivedEvidence ?? []) {
    const full = join(root, evidencePath);
    if (!existsSync(full)) failures.push(`missing archived evidence ${evidencePath}`);
  }
  return { id: fixture.id, origin: 'historical-import', available: true, ok: failures.length === 0,
    sourceSha256: sourceHash.sha256, sourceFiles: sourceHash.files.length,
    archivedEvidence: fixture.archivedEvidence?.length ?? 0, failures };
}

function findFiles(root, name) {
  const found = [];
  const walk = directory => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name === name) found.push(relative(root, path));
    }
  };
  walk(resolve(root));
  return found;
}

function isSafeRelativePath(path) {
  if (typeof path !== 'string' || !path || resolve(path) === path) return false;
  const normalized = resolve(ROOT, path);
  return normalized.startsWith(`${ROOT}${sep}`) && !path.split(/[\\/]/).includes('..');
}

function readJson(path, label, failures) {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { failures.push(`${label} is not valid JSON: ${error.message}`); return null; }
}

async function main() {
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  const candidates = registry.fixtures.filter(fixture => fixture.status === 'candidate')
    .map(fixture => inspectReferenceCandidate(fixture));
  const imports = registry.fixtures.filter(fixture => fixture.status !== 'blocked')
    .map(fixture => inspectImportedReference(fixture));
  console.log(JSON.stringify({ validation, candidates, imports }, null, 2));
  // Historical origin trees live in ignored result archives and may not be in
  // a clean checkout. Verify them when present, but gate CI on the canonical,
  // checked-in import whose exact hash and hygiene are the durable contract.
  if (!validation.ok || candidates.some(candidate => candidate.available && !candidate.ok)
    || imports.some(item => !item.ok)) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 2; });
}
