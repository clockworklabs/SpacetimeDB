import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import {
  compileFixtureDefinition,
  compilePackDefinition,
  compilePromotionFile,
  compileRecipeFile,
} from './composition-compiler.mjs';
import { TRACKS_DIR, listTracks } from './tracks.mjs';

function json(path) {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { throw new Error(`cannot read composition source ${path}: ${error.message}`, { cause: error }); }
}

export function checkCompositions({ trackName = null } = {}) {
  const names = trackName ? [trackName] : listTracks({ includeInternal: true });
  const summary = [];
  for (const name of names) {
    const trackRoot = join(TRACKS_DIR, name);
    const root = join(trackRoot, 'composition');
    if (!existsSync(root)) {
      if (trackName) throw new Error(`track ${name} has no composition directory`);
      continue;
    }
    const packs = join(root, 'packs');
    const fixtures = join(root, 'fixtures');
    const recipes = join(root, 'recipes');
    const packFiles = readdirSync(packs).filter(file => file.endsWith('.json')).sort();
    const fixtureFiles = readdirSync(fixtures).filter(file => file.endsWith('.json')).sort();
    const recipeFiles = readdirSync(recipes).filter(file => file.endsWith('.json')).sort();
    if (!packFiles.length || !fixtureFiles.length || !recipeFiles.length) {
      throw new Error(`track ${name} composition must contain packs, fixtures, and recipes`);
    }
    for (const file of packFiles) {
      const path = join(packs, file);
      compilePackDefinition(json(path), { source: path });
    }
    for (const file of fixtureFiles) {
      const path = join(fixtures, file);
      compileFixtureDefinition(json(path), { source: path });
    }
    const plans = recipeFiles.map(file => compileRecipeFile(join(recipes, file), { trackRoot }));
    const promotionPath = join(root, 'promotions.json');
    const promotion = existsSync(promotionPath)
      ? compilePromotionFile(promotionPath, { trackRoot }) : null;
    summary.push({ track: name, packs: packFiles.length, fixtures: fixtureFiles.length,
      recipes: plans.length, checks: plans.reduce((total, plan) => total + plan.checks.length, 0),
      aliases: promotion?.entries.length ?? 0 });
  }
  return summary;
}

function main() {
  const args = process.argv.slice(2);
  let trackName = null;
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === '--track' && args[index + 1]) trackName = args[++index];
    else throw new Error(`unknown or incomplete argument ${args[index]}`);
  }
  const summary = checkCompositions({ trackName });
  if (!summary.length) throw new Error('no composition sources found');
  for (const row of summary) {
    console.log(`${row.track}: ${row.packs} packs, ${row.fixtures} fixtures, ${row.recipes} recipes, ${row.checks} selected checks, ${row.aliases} alias candidates`);
  }
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) main();
