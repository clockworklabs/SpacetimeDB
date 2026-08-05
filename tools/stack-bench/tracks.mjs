// A track is one application the benchmark can build and grade: its level
// prompts, its UI contract, its scenario suites, and the golden path its
// contract linter walks.
//
// Everything that differs between applications lives inside tracks/<name>/, so
// adding one is a matter of dropping in a directory with a track.json — no
// change to the harness. Everything that differs between *backends* stays in
// backends/, because that axis is what the benchmark measures.

import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
export const TRACKS_DIR = join(ROOT, 'tracks');
export const DEFAULT_TRACK = 'chat';

export function listTracks() {
  if (!existsSync(TRACKS_DIR)) return [];
  return readdirSync(TRACKS_DIR, { withFileTypes: true })
    .filter(e => e.isDirectory() && existsSync(join(TRACKS_DIR, e.name, 'track.json')))
    .map(e => e.name)
    .sort();
}

export function loadTrack(name = DEFAULT_TRACK) {
  const dir = join(TRACKS_DIR, name);
  const manifest = join(dir, 'track.json');
  if (!existsSync(manifest)) {
    console.error(`Unknown track "${name}". Available: ${listTracks().join(', ') || 'none'}`);
    process.exit(2);
  }
  const m = JSON.parse(readFileSync(manifest, 'utf8'));
  return {
    name,
    dir,
    // Substituted into the backend guidance docs, so the app is branded for the
    // application being built rather than always for chat.
    title: m.title ?? name,
    // Ports, database names and result directories are derived from this, so
    // two tracks at the same --run-index cannot collide. Empty for chat, whose
    // names are the ones every existing result was recorded under.
    slug: m.slug ?? '',
    portOffset: m.portOffset ?? 0,
    // What to poll after restarting the app's server to know it is back.
    restartProbe: m.restartProbe ?? '/',
    // Set when the application seeds fixture data at startup: wiping the
    // database also wipes the seed, so the server has to be restarted before
    // grading can assume it is there.
    reseedOnReset: m.reseedOnReset ?? false,
    suites: m.suites ?? {},
    prompts: join(dir, 'prompts'),
    contracts: join(dir, 'contracts'),
    scenarios: join(dir, 'scenarios'),
    walk: join(dir, 'walk.mjs'),
  };
}

// Suffixed names: chat keeps the unsuffixed originals, so its databases,
// modules and result directories are exactly what they have always been.
export const dbName = (track, runIndex) =>
  `stackbench${track.slug ? `_${track.slug}` : ''}_run${runIndex}`;

export const moduleName = (track, runIndex) =>
  `stackbench${track.slug ? `-${track.slug}` : ''}-run${runIndex}`;

export const resultsName = (track, backend, runIndex) =>
  `${backend}${track.slug ? `-${track.slug}` : ''}-run${runIndex}`;

export function levelPrompt(track, level) {
  const prefix = String(level).padStart(2, '0') + '-';
  const file = existsSync(track.prompts)
    ? readdirSync(track.prompts).find(f => f.startsWith(prefix))
    : null;
  if (!file) throw new Error(`No prompt for level ${level} in ${track.prompts}`);
  return readFileSync(join(track.prompts, file), 'utf8');
}

export function appendix(track, level) {
  const f = join(track.contracts, `appendix-${String(level).padStart(2, '0')}.md`);
  return existsSync(f) ? readFileSync(f, 'utf8') : '';
}

// Suites are declared per level in the manifest, with spec paths relative to
// the track directory. A level with no entry of its own falls back to level 1's,
// which is what grading a higher level did before tracks existed.
export function suitesFor(track, level) {
  const declared = track.suites[String(level)] ?? track.suites['1'] ?? [];
  return declared.map(s => ({ ...s, spec: join(track.dir, s.spec) }));
}
