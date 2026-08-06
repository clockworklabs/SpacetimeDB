#!/usr/bin/env node
// Static check on the scenario files, so a typo costs a second rather than a
// full graded run. Verifies that every step names a `do` the grader implements,
// every testid it touches is a hook the contract actually requires, and every
// actor it names is declared. None of this needs an app.
//
// Usage: node check-scenarios.mjs

import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, listTracks, DEFAULT_TRACK } from '../tracks.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

const trackArg = process.argv.includes('--track')
  ? process.argv[process.argv.indexOf('--track') + 1]
  : null;
const trackNames = trackArg ? [trackArg] : (listTracks().length ? listTracks() : [DEFAULT_TRACK]);

// The step vocabulary, read from the grader itself rather than duplicated here.
const grader = readFileSync(join(HERE, 'grade.mjs'), 'utf8');
// Some steps are dispatched by an early `if` rather than the switch, so read both.
const known = new Set([
  ...[...grader.matchAll(/case '([a-zA-Z]+)':/g)].map(m => m[1]),
  ...[...grader.matchAll(/step\.do === '([a-zA-Z]+)'/g)].map(m => m[1]),
]);

let problems = 0;
const fail = (where, msg) => { console.log(`  ${where}: ${msg}`); problems++; };

// A track's contracts are keyed by level, one file per level whatever its name.
// Levels are CUMULATIVE — a level 2 scenario legitimately drives level 1's
// hooks, because the app it grades still has them — so each level's set is the
// union of every level up to it, the same way the linter loads them.
function hooksByLevel(track) {
  const perFile = {};
  for (const f of readdirSync(track.contracts).filter(f => /^\d\d-.*\.json$/.test(f))) {
    perFile[f.slice(0, 2)] = JSON.parse(readFileSync(join(track.contracts, f), 'utf8')).hooks.map(h => h.id);
  }
  const byLevel = {};
  for (const lvl of Object.keys(perFile)) {
    byLevel[lvl] = new Set(
      Object.entries(perFile).filter(([l]) => l <= lvl).flatMap(([, ids]) => ids));
  }
  return byLevel;
}

for (const name of trackNames) {
 const track = loadTrack(name);
 console.log(`# track: ${name}`);
 const contracts = hooksByLevel(track);
 for (const file of readdirSync(track.scenarios).filter(f => f.endsWith('.json'))) {
  const spec = JSON.parse(readFileSync(join(track.scenarios, file), 'utf8'));
  const level = file.slice(0, 2);
  const hooks = contracts[level] ?? null;

  console.log(`${file}`);
  for (const f of spec.features ?? []) {
    const actors = new Set(f.actors ?? []);
    const steps = [...(f.setup ?? []), ...(f.criteria ?? []).flatMap(c => c.steps ?? [])];
    for (const s of steps) {
      const at = `F${f.id} ${s.do}`;
      if (!known.has(s.do)) fail(at, `unknown step type "${s.do}"`);
      // A step may address a client opened mid-scenario (freshClient), whose
      // name is derived from a declared actor.
      const declared = a => actors.has(a) || [...actors].some(x => a.startsWith(`${x}-`));
      if (s.actor && actors.size && !declared(s.actor)) fail(at, `actor "${s.actor}" is not in the feature's actor list`);
      for (const a of [s.from, s.fromActor]) {
        if (a && actors.size && !declared(a)) fail(at, `actor "${a}" is not in the feature's actor list`);
      }
      if (hooks) {
        for (const id of [s.testid, s.in?.testid]) {
          if (id && !hooks.has(id)) fail(at, `testid "${id}" is not in the contract`);
        }
      }
    }
    const pts = (f.criteria ?? []).reduce((n, c) => n + (c.points ?? 1), 0);
    if (f.max != null && pts !== f.max) fail(`F${f.id}`, `criteria total ${pts} but max says ${f.max}`);
  }
 }
}

console.log(problems ? `\n${problems} problem(s)` : '\nno problems');
process.exit(problems ? 1 : 0);
