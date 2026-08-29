import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { compileFeatureCatalogInput, compileProgressionDefinitionFile }
  from './progression-definition.js';
import type {
  CompiledProgressionDefinition,
  ProgressionInput,
} from './progression-definition.js';

const EXACT_REF = /^([a-z][a-z0-9]*(?:[._:-][a-z0-9]+)*)@(\d+\.\d+\.\d+)$/;
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
  const match = EXACT_REF.exec(input);
  if (!match) throw new Error('feature catalog must be an exact id@version reference');
  const id = match[1];
  const version = match[2];
  if (!id || !version) throw new Error('feature catalog must be an exact id@version reference');
  const directory = join(track.dir, 'progression');
  const candidates = readdirSync(directory).filter(name => name.endsWith('.json'))
    .map(name => join(directory, name)).filter(path => {
      const value = readJson(path);
      return object(value) && value.id === id && value.version === version;
    });
  if (candidates.length !== 1) {
    throw new Error(`feature catalog must resolve exactly one ${input} definition in track ${track.name}`);
  }
  const candidate = candidates[0];
  if (!candidate) throw new Error(`feature catalog ${input} was not found`);
  return compileFeatureCatalogInput(compileProgressionDefinitionFile(candidate, {
    trackRoot: track.dir,
  }));
}
