#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { compileCalibrationDefinition, compileCalibrationFile } from '../src/composition/calibration-compiler.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { listTracks, TRACKS_DIR } from '../src/composition/tracks.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';

export function checkCalibrations({ trackName = null } = {}) {
  const tracks = trackName ? [trackName] : listTracks({ includeInternal: true });
  const results = [];
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

async function main() {
  const trackIndex = process.argv.indexOf('--track');
  const trackName = trackIndex >= 0 ? process.argv[trackIndex + 1] : null;
  const unknown = process.argv.slice(2).filter((value, index, args) =>
    value !== '--track' && args[index - 1] !== '--track');
  if (unknown.length || (trackIndex >= 0 && !trackName)) {
    throw new Error('usage: node commands/check-calibration.mjs [--track <name>]');
  }
  const results = checkCalibrations({ trackName });
  for (const result of results) {
    console.log(`${result.track}: ${result.id}@${result.version} ${result.state}; ` +
      `${result.controls} controls, ${result.stacks} stacks, ${result.contentSha256.slice(0, 12)}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 1; });
}
