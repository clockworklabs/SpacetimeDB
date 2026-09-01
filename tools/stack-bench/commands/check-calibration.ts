#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { compileCalibrationDefinition, compileCalibrationFile } from '../src/composition/calibration-compiler.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { listTracks, TRACKS_DIR } from '../src/composition/tracks.js';

import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';

export interface CalibrationCheckResult {
  track: string;
  id: string;
  version: string;
  state: string;
  recipe: string;
  controls: number;
  stacks: number;
  contentSha256: string;
}

export function checkCalibrations(
  { trackName = null }: { trackName?: string | null } = {},
): CalibrationCheckResult[] {
  const availableTracks = listTracks({ includeInternal: true });
  if (trackName && !availableTracks.includes(trackName)) {
    throw new Error(`unknown calibration track ${trackName}`);
  }
  const tracks = trackName ? [trackName] : availableTracks;
  const results: CalibrationCheckResult[] = [];
  for (const name of tracks) {
    const trackRoot = join(TRACKS_DIR, name);
    const directory = join(trackRoot, 'composition', 'calibrations');
    if (!existsSync(directory)) continue;
    for (const file of readdirSync(directory).filter(candidate => candidate.endsWith('.json')).sort()) {
      const path = join(directory, file);
      const source = `composition/calibrations/${file}`;
      const input = JSON.parse(readFileSync(path, 'utf8'));
      const definition = compileCalibrationDefinition(input, { source });
      const recipePath = resolve(dirname(path), definition.recipe.path);
      const release = buildRecipeRelease(recipePath, { trackRoot });
      const plan = compileCalibrationFile(path, { trackRoot, stackBenchRoot: ROOT, release });
      results.push({ track: name, id: plan.id, version: plan.version, state: plan.state,
        recipe: plan.recipe.id, controls: plan.controls.length, stacks: plan.qualification.stacks.length,
        contentSha256: plan.contentSha256 });
    }
  }
  return results;
}

function main() {
  const { values } = parseArgs({ args: process.argv.slice(2), options: {
    track: { type: 'string' },
  }, strict: true, allowPositionals: false });
  const results = checkCalibrations({ trackName: values.track ?? null });
  for (const result of results) {
    console.log(`${result.track}: ${result.id}@${result.version} ${result.state}; ` +
      `${result.controls} controls, ${result.stacks} stacks, ${result.contentSha256.slice(0, 12)}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); }
  catch (error: unknown) {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
    process.exitCode = 1;
  }
}
