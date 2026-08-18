import { readFileSync } from 'node:fs';
import { relative, resolve, sep } from 'node:path';

import {
  DEFINITION_SCHEMA_VERSION,
  compileScenarioDefinition,
  compileTrackManifest,
} from './definition-compiler.mjs';
import { TRACKS_DIR } from './tracks.mjs';

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`cannot read benchmark definition ${path}: ${error.message}`, { cause: error });
  }
}

function containedPath(root, path, label) {
  const resolvedRoot = resolve(root);
  const resolvedPath = resolve(resolvedRoot, path);
  const rel = relative(resolvedRoot, resolvedPath);
  if (rel === '..' || rel.startsWith(`..${sep}`)) {
    throw new Error(`${label} escapes track directory: ${path}`);
  }
  return { path: resolvedPath, relative: rel.replaceAll('\\', '/') };
}

// Object key order is not semantic JSON. Array order is retained because suite,
// feature, criterion, step, race-branch, and argument ordering is executable.
export function canonicalizeDefinition(value, at = '$') {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (Array.isArray(value)) {
    return value.map((item, index) => canonicalizeDefinition(item, `${at}[${index}]`));
  }
  if (value && typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype) {
    return Object.fromEntries(Object.keys(value).sort()
      .map(key => [key, canonicalizeDefinition(value[key], `${at}.${key}`)]));
  }
  throw new Error(`definition plan at ${at} is not canonical JSON data`);
}

export function canonicalDefinitionJson(value) {
  return `${JSON.stringify(canonicalizeDefinition(value), null, 2)}\n`;
}

export function compileTrackPlan(name, { tracksDir = TRACKS_DIR } = {}) {
  if (typeof name !== 'string' || !/^[a-z0-9][a-z0-9_-]*$/.test(name)) {
    throw new Error(`invalid track name ${JSON.stringify(name)}`);
  }
  const track = containedPath(tracksDir, name, 'track').path;
  const manifestPath = containedPath(track, 'track.json', 'manifest').path;
  const manifest = compileTrackManifest(readJson(manifestPath), { source: manifestPath });
  const scenarios = new Map();
  const levels = [];

  for (const level of Object.keys(manifest.suites).map(Number).sort((a, b) => a - b)) {
    const suites = manifest.suites[String(level)].map(suite => {
      const spec = containedPath(track, suite.spec, `L${level} suite ${suite.id}`);
      if (!scenarios.has(spec.relative)) {
        scenarios.set(spec.relative, compileScenarioDefinition(readJson(spec.path), { source: spec.path }));
      }
      return { id: suite.id, inherit: suite.inherit, scenario: spec.relative };
    });
    levels.push({ level, suites });
  }

  return canonicalizeDefinition({
    definitionSchemaVersion: DEFINITION_SCHEMA_VERSION,
    track: name,
    manifest,
    levels,
    scenarios: [...scenarios.entries()].sort(([a], [b]) => a.localeCompare(b))
      .map(([path, definition]) => ({ path, definition })),
  });
}
