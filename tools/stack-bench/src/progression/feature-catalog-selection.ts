import { readFileSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';

import { compileFeatureCatalogInput, compileProgressionDefinitionFile }
  from './progression-definition.js';
import type {
  CompiledProgressionDefinition,
  ProgressionInput,
} from './progression-definition.js';

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8')) as unknown;

export interface FeatureCatalogTrack {
  name: string;
  dir: string;
}

export function resolveFeatureCatalog(input: unknown,
  track: FeatureCatalogTrack): ProgressionInput<CompiledProgressionDefinition> {
  if (typeof input !== 'string') return compileFeatureCatalogInput(input);
  if (isAbsolute(input)) throw new Error('feature catalog path must be relative to the track');
  const root = resolve(track.dir);
  const path = resolve(root, input);
  const rel = relative(root, path);
  if (rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
    throw new Error('feature catalog path escapes the track');
  }
  const value = readJson(path);
  if (!object(value) || typeof value.id !== 'string') {
    throw new Error(`feature catalog path ${input} has no stable id`);
  }
  return compileFeatureCatalogInput(compileProgressionDefinitionFile(path, {
    trackRoot: track.dir,
  }));
}
