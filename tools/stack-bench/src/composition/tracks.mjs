// A track is one application the benchmark can build and grade: its level
// prompts, its UI contract, its scenario suites, and the golden path its
// contract linter walks.
//
// Everything that differs between applications lives inside tracks/<name>/, so
// adding one is a matter of dropping in a directory with a track.json — no
// change to the harness. Everything that differs between *backends* stays in
// backends/, because that axis is what the benchmark measures.

import { readFileSync, readdirSync, existsSync, statSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { compileTrackManifest } from './definition-compiler.mjs';
import { executeStackCapability } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY, stackPortAllocations } from '../stacks/stack-adapters.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../project-paths.mjs';
export const TRACKS_DIR = join(ROOT, 'tracks');
export const DEFAULT_TRACK = 'chat';

export function listTracks({ includeInternal = false } = {}) {
  if (!existsSync(TRACKS_DIR)) return [];
  return readdirSync(TRACKS_DIR, { withFileTypes: true })
    .filter(e => e.isDirectory() && existsSync(join(TRACKS_DIR, e.name, 'track.json')))
    .filter(e => includeInternal || !JSON.parse(readFileSync(join(TRACKS_DIR, e.name, 'track.json'), 'utf8')).internal)
    .map(e => e.name)
    .sort();
}

export function loadTrack(name = DEFAULT_TRACK) {
  const dir = join(TRACKS_DIR, name);
  const manifest = join(dir, 'track.json');
  if (!existsSync(manifest)) {
    console.error(`Unknown track "${name}". Available: ${listTracks({ includeInternal: true }).join(', ') || 'none'}`);
    process.exit(2);
  }
  const m = compileTrackManifest(JSON.parse(readFileSync(manifest, 'utf8')), { source: manifest });
  return {
    name,
    dir,
    schemaVersion: m.schemaVersion,
    // Substituted into the backend guidance docs, so the app is branded for the
    // application being built rather than always for chat.
    title: m.title ?? name,
    // Ports, database names and result directories are derived from this, so
    // two tracks at the same --run-index cannot collide. Empty for chat, whose
    // names are the ones every existing result was recorded under.
    slug: m.slug ?? '',
    internal: !!m.internal,
    // Only these levels may be used as a published baseline. Higher declared
    // levels are development material until their reference and mutation gates
    // pass; plannedThrough keeps the intended ladder visible.
    validatedThrough: m.validatedThrough ?? 0,
    plannedThrough: m.plannedThrough ?? Math.max(...Object.keys(m.suites ?? {}).map(Number), 0),
    portOffset: m.portOffset ?? 0,
    // What to poll after restarting the app's server to know it is back.
    restartProbe: m.restartProbe ?? '/',
    // Set when the application seeds fixture data at startup: wiping the
    // database also wipes the seed, so the server has to be restarted before
    // grading can assume it is there.
    reseedOnReset: m.reseedOnReset ?? false,
    suites: m.suites ?? {},
    actions: m.actions ?? [],
    prompts: join(dir, 'prompts'),
    contracts: join(dir, 'contracts'),
    scenarios: join(dir, 'scenarios'),
    walk: join(dir, 'walk.mjs'),
  };
}

export function isDeclaredLevel(track, level) {
  return Number.isInteger(level) && level >= 1 && Boolean(track?.suites?.[String(level)]);
}

// ─── Ports ───────────────────────────────────────────────────────────────────
//
// The single source of truth. These used to live separately in agent.mjs and
// bench.mjs, and both Express backends sat on one base — which twice produced a
// client quietly proxying into the OTHER backend's server, and confident scores
// for the wrong database. Bases are spaced per backend, tracks are offset from
// one another, and assertNoPortCollisions() proves the whole grid disjoint
// rather than trusting anyone's arithmetic.

export const PORT_BASES = Object.freeze(stackPortAllocations());

// Run indexes above this are refused: the spacing proof below only covers this
// range, and an uncapped index walks one backend's window into another's — at
// 28 and above, chat's postgres client window reaches ecommerce's postgres API
// base. Twenty concurrent runs of one backend on one machine is already far
// beyond anything this benchmark does.
export const RUN_INDEX_CAP = 20;

export function portsFor(track, backend, runIndex) {
  if (runIndex > RUN_INDEX_CAP) {
    throw new Error(`--run-index ${runIndex} exceeds ${RUN_INDEX_CAP}; the port grid is only proven collision-free below that`);
  }
  const adapter = STACK_ADAPTER_REGISTRY.get(backend);
  return executeStackCapability(adapter, 'ports', 'for-run', {
    trackOffset: track.portOffset,
    runIndex,
  });
}

// Every (track, backend, run-index) combination must own its ports outright.
// Run at startup: a new track whose offset collides with an existing window
// fails loudly here, instead of silently grading the wrong application.
export function assertNoPortCollisions() {
  const owner = new Map();
  // The database containers' host ports are fixed and shared by design, but no
  // track window may land on them either.
  for (const [backend, base] of Object.entries(PORT_BASES)) {
    if (base.db) owner.set(base.db, `${backend} database container`);
  }
  for (const name of listTracks({ includeInternal: true })) {
    const track = loadTrack(name);
    for (const backend of STACK_ADAPTER_REGISTRY.ids) {
      for (let i = 0; i <= RUN_INDEX_CAP; i++) {
        const p = portsFor(track, backend, i);
        for (const port of [p.vite, p.express]) {
          if (port == null) continue;
          const who = `${name}/${backend}/run${i}`;
          if (owner.has(port)) {
            throw new Error(`port ${port} is claimed by both ${owner.get(port)} and ${who} — adjust the new track's portOffset`);
          }
          owner.set(port, who);
        }
      }
    }
  }
}

// Where a build actually happens.
//
// Building inside results/ put the app underneath the harness that grades it,
// so every escape was a walk upward: two directories from the app sit the
// scenario files and grade.mjs. An isolated root removes the class — there is
// nothing above the app worth reading.
//
// Derived from the platform rather than a fixed path, so this works on a Linux
// runner as well as here. STACK_BENCH_WORK_DIR overrides it; point it at the
// same volume as the repo if copying results back matters more than isolation.
//
// The platform temp directory is also considerably shorter than the repo path,
// which buys headroom against the Windows 260-character limit that deep
// node_modules trees run into.
export function workRoot() {
  return process.env.STACK_BENCH_WORK_DIR ?? join(tmpdir(), 'stack-bench-runs');
}

// A run gets its OWN directory, stamped, rather than reusing one per backend.
// Reuse meant a single stale handle wedged every future run: a finished run left
// postgres-run0/app empty but undeletable — some process still had it as its
// working directory — and the next build died on EBUSY after five retries, which
// turns somebody else's leftover into a failed benchmark. A run that never
// reuses a path cannot be blocked by one.
//
// `stamp` is supplied by the caller so every level of one run shares a directory
// — L2 upgrades the app L1 built.
export const workDirFor = (track, backend, runIndex, stamp) =>
  join(workRoot(), resultsName(track, backend, runIndex) + (stamp ? `-${stamp}` : ''));

// Old run directories are deleted on the way in, so finished work does not
// accumulate in temp forever. Best-effort by design: one locked leftover must
// not stop the run that is starting. Returns what it could not remove, for the
// caller to say out loud rather than hide.
export function sweepWorkRoot(maxAgeHours = 12) {
  const root = workRoot();
  const stuck = [];
  if (!existsSync(root)) return stuck;
  const cutoff = Date.now() - maxAgeHours * 3600_000;
  for (const name of readdirSync(root)) {
    const dir = join(root, name);
    try {
      if (statSync(dir).mtimeMs > cutoff) continue;
      rmSync(dir, { recursive: true, force: true });
    } catch { stuck.push(dir); }
  }
  return stuck;
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
// the track directory. Missing declarations are an incomplete level, not a
// reason to silently grade a higher-level prompt with L1's feature suite.
// Guarantees, unlike features, are promises that must not break later. A level
// adds features; it must not cost you the invariants the earlier levels earned.
// These suites are therefore re-run at every level above the one that
// introduced them.
//
// This is the measurement the ladder exists for. Cost per level alone cannot
// distinguish "grew the app" from "grew the app and quietly broke live stock
// updates"; a stack that maintains propagation by hand pays for every new write
// path, and the failure shows up here rather than in the feature score. Without
// it, L3 never re-checks L1's promises and a regression is invisible.
export function suitesFor(track, level) {
  const at = lvl => (track.suites[String(lvl)] ?? []).map(s => ({ ...s, spec: join(track.dir, s.spec) }));
  if (!track.suites[String(level)]) {
    throw new Error(`No scenario suites declared for ${track.name} level ${level}`);
  }
  const declared = at(level);

  // Earlier levels' guarantee suites, oldest first, deduped by spec: a suite
  // declared at several levels (01-contention is listed at 1 and 2) is one
  // suite, and the current level's own declaration always wins so it keeps its
  // plain id and its points.
  const seen = new Set(declared.map(s => s.spec));
  const inherited = [];
  for (let lvl = 1; lvl < level; lvl++) {
    for (const s of at(lvl)) {
      if (s.inherit !== 'all-higher-levels' || seen.has(s.spec)) continue;
      seen.add(s.spec);
      // A distinct id, because the bundle keys suites by id and a collision
      // would silently overwrite one result with another.
      inherited.push({ ...s, id: `${s.id}@L${lvl}`, inherited: true, fromLevel: lvl });
    }
  }
  return [...declared, ...inherited];
}
