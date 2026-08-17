#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync,
  writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hashDirectory } from './provenance.mjs';
import { seedAppSource } from './source-snapshot.mjs';

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

export function selectReferenceFixture(registry, { backend, track, level, recipe } = {}) {
  const inScope = registry.fixtures.filter(fixture => fixture.backend === backend
    && fixture.track === track && fixture.level === level);
  const recipeScoped = recipe
    ? inScope.filter(fixture => fixture.recipes?.includes(recipe))
    : [];
  const matches = recipeScoped.length
    ? recipeScoped.filter(fixture => fixture.status !== 'blocked')
    : inScope.filter(fixture => fixture.status === 'active' && !fixture.recipes?.length);
  if (matches.length !== 1) {
    throw new Error(`reference source requires exactly one ${track} L${level} ${backend} fixture for ${recipe ?? 'the default recipe'}`);
  }
  return matches[0];
}

export function validateReferenceRegistry(registry, { root = ROOT } = {}) {
  const issues = [];
  if (registry?.schemaVersion !== 4) issues.push('schemaVersion must be 4');
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
    const recipes = fixture.recipes ?? [];
    if (!Array.isArray(recipes) || recipes.some(recipe => typeof recipe !== 'string'
        || !/^[a-z0-9][a-z0-9._-]*@(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/.test(recipe))) {
      issues.push(`${label}: recipes must contain exact recipe identities`);
    }
    for (const recipe of recipes.length ? recipes : ['<default>']) {
      const tuple = `${fixture.track}:${fixture.backend}:${fixture.level}:${recipe}`;
      if (tuples.has(tuple)) issues.push(`${label}: duplicate track/backend/level/recipe`);
      tuples.add(tuple);
    }
    if (fixture.source) {
      if (!recipes.length) issues.push(`${label}: derived source requires at least one recipe selector`);
      if (!safeReferencePath(fixture.source.basePath)
          || !safeReferencePath(fixture.source.patchPath)) {
        issues.push(`${label}: derived source paths must stay under reference-apps/`);
      }
      if (!/^[a-f0-9]{64}$/.test(fixture.source.baseSha256 ?? '')) {
        issues.push(`${label}: derived source must bind its base hash`);
      }
    } else if (typeof fixture.targetPath !== 'string' || !fixture.targetPath.startsWith('reference-apps/')) {
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
    const importedPathOk = fixture.source
      ? fixture.imported?.path === undefined
      : fixture.imported?.path === fixture.targetPath;
    if (!fixture.imported || !importedPathOk
        || !/^[a-f0-9]{64}$/.test(fixture.imported.sourceSha256 ?? '')) {
      issues.push(`${label}: imported sourceSha256 and source location are required`);
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
  const failures = [];
  let source;
  try { source = effectiveReferenceSource(fixture, { root }); }
  catch (error) {
    return { id: fixture.id, available: false, ok: false, failures: [error.message] };
  }
  const sourceHash = hashEffectiveFiles(source.files);
  if (sourceHash.sha256 !== fixture.imported?.sourceSha256) failures.push('imported fixture hash does not match registry');
  let metadata;
  const metadataBytes = source.files.get('reference.json');
  if (!metadataBytes) failures.push('reference.json is missing');
  else {
    try { metadata = JSON.parse(metadataBytes.toString('utf8')); }
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
          const packageName = `${directory}/package.json`.replaceAll('\\', '/');
          const lockName = `${directory}/package-lock.json`.replaceAll('\\', '/');
          if (!source.files.has(packageName)) failures.push(`${packageName} is missing`);
          else readJsonBytes(source.files.get(packageName), packageName, failures);
          if (!source.files.has(lockName)) failures.push(`${lockName} is missing`);
          else {
            const lock = readJsonBytes(source.files.get(lockName), lockName, failures);
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
    const bytes = source.files.get(name);
    if (!bytes.includes(0)) {
      const contents = bytes.toString('utf8');
      if (WORKSTATION_PATHS.some(pattern => pattern.test(contents))) failures.push(`workstation absolute path in ${name}`);
      if (TRUNCATION_MARKER.test(contents)) failures.push(`tool-output truncation marker in ${name}`);
    }
  }
  return { id: fixture.id, available: true, ok: failures.length === 0,
    sourceSha256: sourceHash.sha256, sourceFiles: sourceHash.files.length, failures };
}

export function prepareReferenceFixtureSource(fixture, destination, { root = ROOT } = {}) {
  const source = effectiveReferenceSource(fixture, { root });
  seedAppSource(source.basePath, destination);
  for (const edit of source.edits) {
    const path = join(destination, ...edit.path.split('/'));
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, source.files.get(edit.path));
  }
  return hashDirectory(destination);
}

export function assertPlainReferenceSourceTree(source) {
  if (!existsSync(source)) return;
  const root = lstatSync(source);
  if (!root.isDirectory()) throw new Error('reference source root is not a plain directory');
  const walk = directory => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      const name = relative(source, path).replaceAll('\\', '/');
      if (entry.isDirectory()) walk(path);
      else if (!entry.isFile()) throw new Error(`reference source contains unsupported filesystem entry ${name}`);
    }
  };
  walk(source);
}

function effectiveReferenceSource(fixture, { root }) {
  const baseRelative = fixture.source?.basePath ?? fixture.imported?.path ?? fixture.targetPath;
  if (!safeReferencePath(baseRelative)) throw new Error('imported fixture path is unsafe');
  const basePath = join(root, baseRelative ?? '');
  if (!existsSync(basePath)) throw new Error('imported fixture directory is missing');
  assertContainedPlainTree(root, basePath);
  const baseHash = hashDirectory(basePath);
  if (fixture.source?.baseSha256 && baseHash.sha256 !== fixture.source.baseSha256) {
    throw new Error('derived fixture base hash does not match registry');
  }
  const files = new Map(baseHash.files.map(name => [name,
    readFileSync(join(basePath, ...name.split('/')))]));
  if (fixture.source && !safeReferencePath(fixture.source.patchPath)) {
    throw new Error('derived fixture patch path is unsafe');
  }
  const edits = fixture.source
    ? loadSourceEdits(join(root, fixture.source.patchPath), files, { root }) : [];
  for (const edit of edits) files.set(edit.path, edit.bytes);
  return { basePath, edits, files };
}

function loadSourceEdits(path, files, { root }) {
  if (!existsSync(path)) throw new Error('derived fixture patch file is missing');
  const realRoot = realpathSync(root);
  const realPatch = realpathSync(path);
  if (!containedRealPath(realRoot, realPatch) || !lstatSync(path).isFile()) {
    throw new Error('derived fixture patch file is not a plain contained file');
  }
  let patch;
  try { patch = JSON.parse(readFileSync(path, 'utf8')); }
  catch { throw new Error('derived fixture patch file is not valid JSON'); }
  if (patch?.schemaVersion !== 1 || !Array.isArray(patch.edits) || patch.edits.length === 0) {
    throw new Error('derived fixture patch must contain versioned edits');
  }
  const working = new Map(files);
  const touched = new Set();
  for (const [index, edit] of patch.edits.entries()) {
    if (!isSafeSourceFile(edit?.path) || typeof edit.find !== 'string' || !edit.find
        || typeof edit.replace !== 'string') {
      throw new Error(`derived fixture edit ${index} is invalid`);
    }
    touched.add(edit.path);
    const original = working.get(edit.path);
    if (!original) throw new Error(`derived fixture edit target ${edit.path} is missing`);
    const text = original.toString('utf8');
    const occurrences = text.split(edit.find).length - 1;
    if (occurrences !== 1) {
      throw new Error(`derived fixture edit ${edit.path} expected one anchor, found ${occurrences}`);
    }
    const bytes = Buffer.from(text.replace(edit.find, edit.replace));
    working.set(edit.path, bytes);
  }
  return [...touched].map(pathName => ({ path: pathName, bytes: working.get(pathName) }));
}

function hashEffectiveFiles(files) {
  const names = [...files.keys()].sort((a, b) => a.localeCompare(b));
  const hash = createHash('sha256');
  for (const name of names) {
    const bytes = files.get(name);
    hash.update(`${name.length}:${name}:${bytes.length}:`);
    hash.update(bytes);
  }
  return { sha256: hash.digest('hex'), files: names };
}

function safeReferencePath(path) {
  return isSafeRelativePath(path) && path.replaceAll('\\', '/').startsWith('reference-apps/');
}

function isSafeSourceFile(path) {
  return isSafeRelativePath(path);
}

function assertContainedPlainTree(root, source) {
  const realRoot = realpathSync(root);
  const realSource = realpathSync(source);
  if (!containedRealPath(realRoot, realSource)) {
    throw new Error('reference source resolves outside its registry root');
  }
  assertPlainReferenceSourceTree(source);
}

function containedRealPath(root, target) {
  return target === root || target.startsWith(`${root}${sep}`);
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

function readJsonBytes(bytes, label, failures) {
  try { return JSON.parse(bytes.toString('utf8')); }
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
