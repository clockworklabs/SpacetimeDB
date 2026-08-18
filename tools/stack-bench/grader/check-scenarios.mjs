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
import { loadTrack, listTracks, DEFAULT_TRACK } from '../src/composition/tracks.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

const trackArg = process.argv.includes('--track')
  ? process.argv[process.argv.indexOf('--track') + 1]
  : null;
const trackNames = trackArg ? [trackArg] : (listTracks().length ? listTracks() : [DEFAULT_TRACK]);

const known = new Set(ACTION_REGISTRY.ids);

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

// `statedBy` predates modular prompt treatments. It remains useful provenance,
// but it is no longer a universal launch invariant: a specification can be
// intentionally graded in an unmentioned treatment, and requested treatments
// are assembled from pack-owned prompt fragments rather than only the legacy
// level prompt. Keep stale legacy quotes visible without confusing them with an
// executable scenario error. Pack composition and prompt snapshots validate the
// actual requested/unmentioned treatment bindings.
import { existsSync } from 'node:fs';
const norm = t => t.replace(/\*\*/g, '').replace(/—/g, '-').toLowerCase().replace(/\s+/g, ' ').trim();
function promptFor(track, level) {
  const dir = join(track.dir, 'prompts');
  if (!existsSync(dir)) return null;
  const f = readdirSync(dir).find(f => f.startsWith(level + '-') && f.endsWith('.md'));
  return f ? norm(readFileSync(join(dir, f), 'utf8')) : null;
}

let unstatedWarnings = 0;
let staleStatementWarnings = 0;

for (const name of trackNames) {
 const track = loadTrack(name);
 console.log(`# track: ${name}`);
 const contracts = hooksByLevel(track);
 for (const file of readdirSync(track.scenarios).filter(f => f.endsWith('.json'))) {
  const scenarioPath = join(track.scenarios, file);
  let spec;
  try {
    spec = compileScenarioDefinition(JSON.parse(readFileSync(scenarioPath, 'utf8')),
      { source: scenarioPath });
  } catch (error) {
    fail(file, error.message);
    continue;
  }
  const level = String(spec.level).padStart(2, '0');
  const hooks = contracts[level] ?? null;

  console.log(`${file}`);
  const prompt = promptFor(track, level);
  for (const f of spec.features ?? []) {
    for (const c of f.criteria ?? []) {
      if (c.statedBy) {
        if (prompt && !prompt.includes(norm(c.statedBy))) {
          staleStatementWarnings++;
          console.log(`  warn F${f.id} ${c.id}: statedBy text is not in the legacy level ${level} prompt - the criterion may use modular prompt treatment`);
        }
      } else if ((c.points ?? 0) > 0) {
        unstatedWarnings++;
        console.log(`  warn F${f.id} ${c.id}: carries ${c.points} point(s) with no statedBy - the requirement may be unstated`);
      }
    }
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

const warnings = unstatedWarnings + staleStatementWarnings;
console.log(problems ? `\n${problems} error(s); ${warnings} warning(s)`
  : warnings
    ? `\n0 errors; ${warnings} warning(s) (${unstatedWarnings} point-carrying criteria lack statedBy; ${staleStatementWarnings} statedBy references are outside legacy prompts)`
    : '\n0 errors; 0 warnings');
process.exit(problems ? 1 : 0);
