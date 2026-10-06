#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { listTracks, TRACKS_DIR } from '../src/composition/tracks.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { progressionLevels, selectFeatureCatalogLevels } from '../src/progression/progression-definition.js';
import { inspectImportedReference, loadReferenceRegistry } from '../src/references/reference-fixtures.js';
import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';
import { checkCalibrations } from './check-calibration.js';

type Json = Record<string, any>;
const sha256 = (bytes: Buffer | string) => createHash('sha256').update(bytes).digest('hex');
const readJson = (path: string): Json => JSON.parse(readFileSync(path, 'utf8'));

// Write only a changed file, in the 2-space form these files already use.
function writeJson(path: string, value: unknown, changed: string[]): void {
  const text = `${JSON.stringify(value, null, 2)}\n`;
  if (readFileSync(path, 'utf8').replaceAll('\r\n', '\n') === text) return;
  writeFileSync(path, text);
  changed.push(path);
}

function lockfiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => entry.name === 'node_modules' ? []
    : entry.isDirectory() ? lockfiles(join(directory, entry.name))
      : entry.name === 'package-lock.json' ? [join(directory, entry.name)] : []);
}

// Recompute every derived hash that sources pin, after the sources change. This
// never edits qualification evidence; check-calibration reports what a change invalidates.
export function repin(): string[] {
  const changed: string[] = [];
  // A release builds its local packages (resolved file:) itself, so a checksum
  // pinned in a reference lockfile only breaks when that package changes.
  for (const path of lockfiles(join(ROOT, 'reference-apps'))) {
    const lock = readJson(path);
    for (const entry of Object.values<Json>(lock.packages ?? {})) {
      if (String(entry.resolved ?? '').startsWith('file:')) delete entry.integrity;
    }
    writeJson(path, lock, changed);
  }

  const registryPath = join(ROOT, 'reference-apps', 'registry.json');
  const registry = readJson(registryPath);
  for (const fixture of loadReferenceRegistry(registryPath).fixtures) {
    if (!fixture.imported) continue;
    const inspected = inspectImportedReference(fixture, { root: ROOT });
    if (!inspected.available || !inspected.sourceSha256) throw new Error(`${fixture.id}: ${inspected.failures.join('; ')}`);
    registry.fixtures.find((entry: Json) => entry.id === fixture.id).imported.sourceSha256 = inspected.sourceSha256;
  }
  writeJson(registryPath, registry, changed);
  const referenceHash = (id: string): string => {
    const hash = registry.fixtures.find((entry: Json) => entry.id === id)?.imported?.sourceSha256;
    if (!hash) throw new Error(`reference ${id} has no imported source hash`);
    return hash;
  };

  for (const track of listTracks({ includeInternal: true })) {
    const trackRoot = join(TRACKS_DIR, track);
    const directory = join(trackRoot, 'composition', 'calibrations');
    if (!existsSync(directory)) continue;
    for (const file of readdirSync(directory).filter(name => name.endsWith('.json')).sort()) {
      const path = join(directory, file);
      const calibration = readJson(path);
      const release = buildRecipeRelease(resolve(dirname(path), calibration.recipe.path), { trackRoot });
      for (const key of ['meaningSha256', 'executionSha256', 'contentSha256']) calibration.recipe[key] = release[key];
      calibration.fixture.sourceSha256 = release.components.fixture.sha256;
      calibration.selection.sha256 = sha256(readFileSync(resolve(trackRoot, calibration.selection.path)));
      const catalog = calibration.qualification.featureCatalog;
      if (catalog) {
        const full = resolveFeatureCatalog(catalog.path, { dir: trackRoot, name: release.track });
        const level = Number(calibration.selection.alias.slice(1));
        catalog.contentSha256 = selectFeatureCatalogLevels(full,
          progressionLevels(full).filter(candidate => candidate <= level)).identity.contentSha256;
      }
      for (const entry of calibration.references.entries) entry.sourceSha256 = referenceHash(entry.id);
      for (const entry of calibration.mutations) {
        const manifestPath = join(ROOT, entry.path);
        const manifest = readJson(manifestPath);
        manifest.fixtureSha256 = referenceHash(entry.referenceId);
        writeJson(manifestPath, manifest, changed);
        entry.sha256 = sha256(readFileSync(manifestPath));
      }
      writeJson(path, calibration, changed);
    }
  }
  checkCalibrations();
  return [...new Set(changed)];
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const changed = repin();
    console.log(changed.length ? changed.map(path => `updated ${path.slice(ROOT.length + 1)}`).join('\n') : 'all pins current');
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
