import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { compileFeatureCatalogInput, compileProgressionDefinitionFile }
  from './progression-definition.mjs';

const EXACT_REF = /^([a-z][a-z0-9]*(?:[._:-][a-z0-9]+)*)@(\d+\.\d+\.\d+)$/;
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

export function resolveFeatureCatalog(input, track) {
  if (typeof input !== 'string') return compileFeatureCatalogInput(input);
  const match = EXACT_REF.exec(input);
  if (!match) throw new Error('feature catalog must be an exact id@version reference');
  const directory = join(track.dir, 'progression');
  const candidates = readdirSync(directory).filter(name => name.endsWith('.json'))
    .map(name => join(directory, name)).filter(path => {
      const value = readJson(path);
      return value.id === match[1] && value.version === match[2];
    });
  if (candidates.length !== 1) {
    throw new Error(`feature catalog must resolve exactly one ${input} definition in track ${track.name}`);
  }
  return compileFeatureCatalogInput(compileProgressionDefinitionFile(candidates[0], {
    trackRoot: track.dir,
  }));
}
