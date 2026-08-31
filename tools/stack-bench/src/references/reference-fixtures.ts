#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hashDirectory, type HashFilesResult } from '../evidence/provenance.js';
import { hashAppSource, seedAppSource } from '../runtime/source-snapshot.js';

import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';

export interface ReferenceFixture {
  id: string;
  backend: string;
  track: string;
  level: number;
  status: string;
  recipes?: string[];
  actionLevels?: number[];
  targetPath?: string;
  imported?: { path?: string; sourceSha256?: string };
  origin?: { kind?: string; note?: string };
  mutationManifests?: string[];
  [key: string]: unknown;
}

export interface ReferenceRegistry {
  fixtures: ReferenceFixture[];
  [key: string]: unknown;
}

export interface ReferenceFixtureSelector {
  backend?: string;
  track?: string;
  level?: number;
  recipe?: string;
}

type ReferenceFixtureSource = Pick<ReferenceFixture, 'imported' | 'targetPath'>;




interface EffectiveSource {
  basePath: string;
  files: Map<string, Buffer>;
}

interface ImportedInspection {
  id: string;
  available: boolean;
  ok: boolean;
  sourceSha256?: string;
  sourceFiles?: number;
  failures: string[];
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const REGISTRY = join(ROOT, 'reference-apps', 'registry.json');
const BACKENDS = new Set(['spacetime', 'postgres', 'mongodb']);
const STATUSES = new Set(['blocked', 'candidate', 'active']);
const FIXTURE_KINDS = new Set(['node-api', 'spacetime']);
const FORBIDDEN_DIRECTORIES = new Set(['node_modules', 'dist', 'module_bindings', 'stack-bench']);
const FORBIDDEN_FILES = [/^\.env(?:\..*)?$/i, /\.mutation-backup(?:\..*)?$/i];
const WORKSTATION_PATHS = [/[A-Z]:[\\/](?:Users|Development)[\\/]/i];
const TRUNCATION_MARKER = /…\d+\s+(?:tokens|chars)\s+truncated…/i;

export function loadReferenceRegistry(path: string = REGISTRY): ReferenceRegistry {
  return JSON.parse(readFileSync(path, 'utf8')) as ReferenceRegistry;
}

export function selectReferenceFixture(registry: ReferenceRegistry,
  { backend, track, level, recipe }: ReferenceFixtureSelector = {}): ReferenceFixture {
  const inScope = registry.fixtures.filter(fixture => fixture.backend === backend
    && fixture.track === track && level !== undefined
    && referenceActionLevels(fixture).includes(level));
  const recipeScoped = recipe
    ? inScope.filter(fixture => fixture.recipes?.includes(recipe))
    : [];
  const matches = recipeScoped.length
    ? recipeScoped.filter(fixture => fixture.status !== 'blocked')
    : inScope.filter(fixture => fixture.status === 'active' && !fixture.recipes?.length);
  const [match] = matches;
  if (matches.length !== 1 || !match) {
    throw new Error(`reference source requires exactly one ${track} L${level} ${backend} fixture for ${recipe ?? 'the default recipe'}`);
  }
  return match;
}

export interface ImportedReferenceFixture extends ReferenceFixture {
  targetPath: string;
  imported: { sourceSha256: string };
}

export function selectImportedReferenceFixture(registry: ReferenceRegistry,
  selector: ReferenceFixtureSelector = {}): ImportedReferenceFixture {
  const fixture = selectReferenceFixture(registry, selector);
  if (typeof fixture.targetPath !== 'string' || !fixture.targetPath
    || typeof fixture.imported?.sourceSha256 !== 'string' || !fixture.imported.sourceSha256) {
    throw new Error(`reference fixture ${fixture.id} requires an imported source and target path`);
  }
  return { ...fixture, targetPath: fixture.targetPath,
    imported: { ...fixture.imported, sourceSha256: fixture.imported.sourceSha256 } };
}

export function validateReferenceRegistry(registry: unknown,
  { root = ROOT }: { root?: string } = {}): { ok: boolean; issues: string[] } {
  const issues: string[] = [];
  const candidate = record(registry) ? registry : {};
  if (candidate.schemaVersion !== 4) issues.push('schemaVersion must be 4');
  const fixtures: ReferenceFixture[] = Array.isArray(candidate.fixtures) ? candidate.fixtures : [];
  if (fixtures.length === 0) {
    return { ok: false, issues: [...issues, 'fixtures must be a non-empty array'] };
  }
  const ids = new Set();
  const tuples = new Set();
  const referencedManifests = new Map();
  for (const fixture of fixtures) {
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
    const actionLevels = fixture.actionLevels ?? [fixture.level];
    if (!Array.isArray(actionLevels) || actionLevels.length === 0
        || actionLevels.some(level => !Number.isInteger(level) || level < 1)
        || new Set(actionLevels).size !== actionLevels.length) {
      issues.push(`${label}: actionLevels must contain unique positive integer levels`);
    } else {
      if (fixture.actionLevels && recipes.length === 0) {
        issues.push(`${label}: actionLevels requires at least one exact recipe selector`);
      }
      if (!actionLevels.includes(fixture.level)
          || actionLevels.some(level => level > fixture.level)) {
        issues.push(`${label}: actionLevels must include and cannot exceed the fixture level`);
      }
      for (const actionLevel of actionLevels) {
        for (const recipe of recipes.length ? recipes : ['<default>']) {
          const tuple = `${fixture.track}:${fixture.backend}:${actionLevel}:${recipe}`;
          if (tuples.has(tuple)) issues.push(`${label}: duplicate track/backend/action-level/recipe`);
          tuples.add(tuple);
        }
      }
    }
    if (fixture.source !== undefined) {
      issues.push(`${label}: source overlays are not supported`);
    }
    if (typeof fixture.targetPath !== 'string' || !fixture.targetPath.startsWith('reference-apps/')) {
      issues.push(`${label}: targetPath must stay under reference-apps/`);
    }
    const manifests: string[] = Array.isArray(fixture.mutationManifests)
      ? fixture.mutationManifests : [];
    if (!Array.isArray(fixture.mutationManifests)) issues.push(`${label}: mutationManifests must be an array`);
    for (const manifestPath of manifests) {
      if (referencedManifests.has(manifestPath)) issues.push(`${label}: ${manifestPath} is already owned by ${referencedManifests.get(manifestPath)}`);
      referencedManifests.set(manifestPath, label);
      const full = join(root, manifestPath);
      if (!existsSync(full)) issues.push(`${label}: missing mutation manifest ${manifestPath}`);
      else {
        const manifest: Record<string, unknown> = JSON.parse(readFileSync(full, 'utf8'));
        if (fixture.status === 'active' || fixture.status === 'candidate') {
          if (manifest.schemaVersion !== 2) {
            issues.push(`${label}: ${manifestPath} must use mutation schema 2`);
          }
          if (manifest.level !== undefined) {
            issues.push(`${label}: ${manifestPath} must not own a level`);
          }
        }
        if (manifest.backend !== fixture.backend || manifest.track !== fixture.track) {
          issues.push(`${label}: ${manifestPath} targets a different backend or track`);
        }
        const expectedStatus = fixture.status === 'active' ? 'active' : 'candidate';
        if (manifest.status !== expectedStatus) {
          issues.push(`${label}: ${manifestPath} must be ${expectedStatus} while fixture is ${fixture.status}`);
        }
        if (fixture.status !== 'blocked' && manifest.fixtureSha256 !== fixture.imported?.sourceSha256) {
          issues.push(`${label}: ${manifestPath} fixtureSha256 must match the imported fixture`);
        }
      }
    }
    if (fixture.status === 'blocked') {
      if (typeof fixture.blockedReason !== 'string' || !fixture.blockedReason) issues.push(`${label}: blockedReason is required`);
      if (manifests.length) issues.push(`${label}: blocked fixtures cannot own mutation manifests`);
      continue;
    }
    const origin = fixture.origin ?? {};
    if (origin.kind !== 'authored') {
      issues.push(`${label}: origin kind must be authored`);
    } else {
      if (typeof origin.note !== 'string' || !origin.note.trim()) {
        issues.push(`${label}: authored origin requires a provenance note`);
      }
    }
    if (!fixture.imported || fixture.imported.path !== fixture.targetPath
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

function referenceActionLevels(fixture: ReferenceFixture): number[] {
  return Array.isArray(fixture.actionLevels) ? fixture.actionLevels : [fixture.level];
}

export function inspectImportedReference(fixture: ReferenceFixture,
  { root = ROOT }: { root?: string } = {}): ImportedInspection {
  const failures: string[] = [];
  let source: EffectiveSource;
  try { source = effectiveReferenceSource(fixture, { root }); }
  catch (error) {
    return { id: fixture.id, available: false, ok: false,
      failures: [errorMessage(error)] };
  }
  const sourceHash = hashEffectiveFiles(source.files);
  if (sourceHash.sha256 !== fixture.imported?.sourceSha256) failures.push('imported fixture hash does not match registry');
  let metadata: unknown;
  const metadataBytes = source.files.get('reference.json');
  if (!metadataBytes) failures.push('reference.json is missing');
  else {
    try { metadata = JSON.parse(metadataBytes.toString('utf8')); }
    catch { failures.push('reference.json is not valid JSON'); }
  }
  if (record(metadata)) {
    if (metadata.schemaVersion !== 1) failures.push('reference.json schemaVersion must be 1');
    if (typeof metadata.kind !== 'string' || !FIXTURE_KINDS.has(metadata.kind)) {
      failures.push('reference.json kind is invalid');
    }
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
            if (record(lock) && (!Number.isInteger(lock.lockfileVersion)
              || Number(lock.lockfileVersion) < 1)) {
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
    if (bytes && !bytes.includes(0)) {
      const contents = bytes.toString('utf8');
      if (WORKSTATION_PATHS.some(pattern => pattern.test(contents))) failures.push(`workstation absolute path in ${name}`);
      if (TRUNCATION_MARKER.test(contents)) failures.push(`tool-output truncation marker in ${name}`);
    }
  }
  return { id: fixture.id, available: true, ok: failures.length === 0,
    sourceSha256: sourceHash.sha256, sourceFiles: sourceHash.files.length, failures };
}

export function prepareReferenceFixtureSource(fixture: ReferenceFixtureSource, destination: string,
  { root = ROOT }: { root?: string } = {}): HashFilesResult {
  const source = effectiveReferenceSource(fixture, { root });
  seedAppSource(source.basePath, destination);
  return hashAppSource(destination);
}

export function assertPlainReferenceSourceTree(source: string): void {
  if (!existsSync(source)) return;
  const root = lstatSync(source);
  if (!root.isDirectory()) throw new Error('reference source root is not a plain directory');
  const walk = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      const name = relative(source, path).replaceAll('\\', '/');
      if (entry.isDirectory()) walk(path);
      else if (!entry.isFile()) throw new Error(`reference source contains unsupported filesystem entry ${name}`);
    }
  };
  walk(source);
}

function effectiveReferenceSource(fixture: ReferenceFixtureSource,
  { root }: { root: string }): EffectiveSource {
  const baseRelative = fixture.imported?.path ?? fixture.targetPath;
  if (!safeReferencePath(baseRelative)) throw new Error('imported fixture path is unsafe');
  const basePath = join(root, baseRelative ?? '');
  if (!existsSync(basePath)) throw new Error('imported fixture directory is missing');
  assertContainedPlainTree(root, basePath);
  const baseHash = hashDirectory(basePath);
  const files = new Map(baseHash.files.map(name => [name,
    readFileSync(join(basePath, ...name.split('/')))]));
  return { basePath, files };
}

function hashEffectiveFiles(files: Map<string, Buffer>): { sha256: string; files: string[] } {
  const names = [...files.keys()].sort((a, b) => a.localeCompare(b));
  const hash = createHash('sha256');
  for (const name of names) {
    const bytes = files.get(name) ?? Buffer.alloc(0);
    hash.update(`${name.length}:${name}:${bytes.length}:`);
    hash.update(bytes);
  }
  return { sha256: hash.digest('hex'), files: names };
}

function safeReferencePath(path: unknown): path is string {
  return isSafeRelativePath(path) && path.replaceAll('\\', '/').startsWith('reference-apps/');
}

function assertContainedPlainTree(root: string, source: string): void {
  const realRoot = realpathSync(root);
  const realSource = realpathSync(source);
  if (!containedRealPath(realRoot, realSource)) {
    throw new Error('reference source resolves outside its registry root');
  }
  assertPlainReferenceSourceTree(source);
}

function containedRealPath(root: string, target: string): boolean {
  return target === root || target.startsWith(`${root}${sep}`);
}

function isSafeRelativePath(path: unknown): path is string {
  if (typeof path !== 'string' || !path || resolve(path) === path) return false;
  const normalized = resolve(ROOT, path);
  return normalized.startsWith(`${ROOT}${sep}`) && !path.split(/[\\/]/).includes('..');
}

function readJsonBytes(bytes: Buffer | undefined, label: string, failures: string[]): unknown {
  try { return JSON.parse((bytes ?? Buffer.alloc(0)).toString('utf8')); }
  catch (error) { failures.push(`${label} is not valid JSON: ${errorMessage(error)}`); return null; }
}

async function main(): Promise<void> {
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  const imports = registry.fixtures.filter(fixture => fixture.status !== 'blocked')
    .map(fixture => inspectImportedReference(fixture));
  console.log(JSON.stringify({ validation, imports }, null, 2));
  if (!validation.ok || imports.some(item => !item.ok)) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
    process.exitCode = 2;
  });
}
