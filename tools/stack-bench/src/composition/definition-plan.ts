import { readFileSync } from 'node:fs';
import { relative, resolve, sep } from 'node:path';

import {
  DEFINITION_SCHEMA_VERSION,
  compileScenarioDefinition,
  compileTrackManifest,
} from './definition-compiler.mjs';
import { TRACKS_DIR } from './tracks.mjs';

export type CanonicalDefinition = null | boolean | number | string
  | CanonicalDefinition[] | { [key: string]: CanonicalDefinition };
type UnknownRecord = Record<string, unknown>;

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value)
  && Object.getPrototypeOf(value) === Object.prototype;

function readJson(path: string): unknown {
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read benchmark definition ${path}: ${message}`, { cause: error });
  }
}

function containedPath(root: string, path: string, label: string): {
  path: string; relative: string;
} {
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
export function canonicalizeDefinition(value: unknown, at = '$'): CanonicalDefinition {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (Array.isArray(value)) {
    return value.map((item, index) => canonicalizeDefinition(item, `${at}[${index}]`));
  }
  if (record(value)) {
    return Object.fromEntries(Object.keys(value).sort()
      .map(key => [key, canonicalizeDefinition(value[key], `${at}.${key}`)]));
  }
  throw new Error(`definition plan at ${at} is not canonical JSON data`);
}

export function canonicalDefinitionJson(value: unknown): string {
  return `${JSON.stringify(canonicalizeDefinition(value), null, 2)}\n`;
}

export function compileTrackPlan(name: string, {
  tracksDir = TRACKS_DIR,
}: { tracksDir?: string } = {}): CanonicalDefinition {
  if (typeof name !== 'string' || !/^[a-z0-9][a-z0-9_-]*$/.test(name)) {
    throw new Error(`invalid track name ${JSON.stringify(name)}`);
  }
  const track = containedPath(tracksDir, name, 'track').path;
  const manifestPath = containedPath(track, 'track.json', 'manifest').path;
  const manifest = compileTrackManifest(readJson(manifestPath), { source: manifestPath });
  const scenarios = new Map<string, unknown>();
  const levels: Array<{ level: number; suites: Array<{
    id: string; inherit: 'none' | 'all-higher-levels'; scenario: string;
  }> }> = [];

  for (const level of Object.keys(manifest.suites).map(Number).sort((a, b) => a - b)) {
    const declaredSuites = manifest.suites[String(level)];
    if (!declaredSuites) throw new Error(`track L${level} has no suites`);
    const suites = declaredSuites.map(suite => {
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
