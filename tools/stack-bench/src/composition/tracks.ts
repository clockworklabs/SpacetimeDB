import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { compileTrackManifest } from './definition-compiler.js';
import type { StackRunPorts } from '../stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY, stackPortAllocations } from '../stacks/stack-adapters.js';

import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';

export interface TrackSuite {
  id: string;
  spec: string;
  inherit?: 'none' | 'all-higher-levels';
  inherited?: boolean;
  fromLevel?: number;
}

export interface TrackDatabaseProvenance {
  action: string;
  markerParameter: string;
  body: Record<string, string>;
}

export interface NamedActionParameter {
  name: string;
  in?: 'body' | 'path';
  placeholder?: string;
  wireType?: string;
}

export interface NamedAction {
  id?: string;
  path?: string;
  method?: string;
  reducer?: string;
  args?: readonly unknown[];
  params?: readonly NamedActionParameter[];
}

export interface TrackAction extends NamedAction {
  id: string;
  path: string;
  reducer: string;
  args: unknown[];
}

export interface Track {
  name: string;
  dir: string;
  schemaVersion: number;
  title: string;
  slug: string;
  internal: boolean;
  validatedThrough: number;
  plannedThrough: number;
  portOffset: number;
  restartProbe: string;
  reseedOnReset: boolean;
  databaseProvenance: TrackDatabaseProvenance | null;
  suites: Record<string, TrackSuite[]>;
  actions: TrackAction[];
  prompts: string;
  contracts: string;
  scenarios: string;
  walk: string;
}

export type TrackDefinition = Track;

// A compiled manifest resolves suites without the paths loadTrack adds.
export interface TrackSuiteSource {
  name: string;
  dir: string;
  suites: Record<string, TrackSuite[]>;
}

export const TRACKS_DIR = join(ROOT, 'tracks');
export const TRACK_MANIFEST_FILE = 'track.json';
export const DEFAULT_TRACK = 'chat';

export function listTracks({ includeInternal = false }: { includeInternal?: boolean } = {}): string[] {
  if (!existsSync(TRACKS_DIR)) return [];
  return readdirSync(TRACKS_DIR, { withFileTypes: true })
    .filter(e => e.isDirectory() && existsSync(join(TRACKS_DIR, e.name, TRACK_MANIFEST_FILE)))
    .filter(e => includeInternal
      || !JSON.parse(readFileSync(join(TRACKS_DIR, e.name, TRACK_MANIFEST_FILE), 'utf8')).internal)
    .map(e => e.name)
    .sort();
}

export function loadTrack(name: string = DEFAULT_TRACK): Track {
  const dir = join(TRACKS_DIR, name);
  const manifest = join(dir, TRACK_MANIFEST_FILE);
  if (!existsSync(manifest)) {
    throw new Error(
      `Unknown track "${name}". Available: ${listTracks({ includeInternal: true }).join(', ') || 'none'}`,
    );
  }
  const m = compileTrackManifest(JSON.parse(readFileSync(manifest, 'utf8')) as unknown,
    { source: manifest });
  return {
    name,
    dir,
    schemaVersion: m.schemaVersion,
    title: m.title ?? name,
    // Distinguishes track-owned ports, databases, and result paths.
    slug: m.slug ?? '',
    internal: !!m.internal,
    // Only validated levels may be published as baselines.
    validatedThrough: m.validatedThrough ?? 0,
    plannedThrough: m.plannedThrough ?? Math.max(...Object.keys(m.suites ?? {}).map(Number), 0),
    portOffset: m.portOffset ?? 0,
    // What to poll after restarting the app's server to know it is back.
    restartProbe: m.restartProbe ?? '/',
    // Set when the application seeds fixture data at startup: wiping the
    // database also wipes the seed, so the server has to be restarted before
    // grading can assume it is there.
    reseedOnReset: m.reseedOnReset ?? false,
    databaseProvenance: m.databaseProvenance ?? null,
    suites: m.suites ?? {},
    actions: (m.actions ?? []) as TrackAction[],
    prompts: join(dir, 'prompts'),
    contracts: join(dir, 'contracts'),
    scenarios: join(dir, 'scenarios'),
    walk: join(ROOT, 'dist', 'tracks', name, 'walk.js'),
  };
}

export function isDeclaredLevel(track: TrackDefinition | null | undefined, level: number): boolean {
  return Number.isInteger(level) && level >= 1 && Boolean(track?.suites?.[String(level)]);
}

export const PORT_BASES = Object.freeze(stackPortAllocations());

// Keep run ports inside the collision-free range checked below.
export const RUN_INDEX_CAP = 20;

export function portsFor(track: TrackDefinition, backend: string, runIndex: number): StackRunPorts {
  if (!Number.isInteger(runIndex) || runIndex < 0 || runIndex > RUN_INDEX_CAP) {
    throw new Error(`--run-index must be an integer from 0 through ${RUN_INDEX_CAP}`);
  }
  const adapter = STACK_ADAPTER_REGISTRY.get(backend);
  return adapter.ports.forRun({
    trackOffset: track.portOffset,
    runIndex,
  });
}

// Every (track, backend, run-index) combination must own its ports outright.
// Run at startup: a new track whose offset collides with an existing window
// fails loudly here, instead of silently grading the wrong application.
export function assertNoPortCollisions(): void {
  const owner = new Map<number, string>();
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

// Keep builds outside the harness tree and below the platform path limit.
export function workRoot(): string {
  return process.env.STACK_BENCH_WORK_DIR ?? join(tmpdir(), 'stack-bench-runs');
}

// A stamped directory isolates each run while allowing its levels to share the
// same application source for sequential upgrades.
export const workDirFor = (track: TrackDefinition, backend: string, runIndex: number,
  stamp?: string): string =>
  join(workRoot(), resultsName(track, backend, runIndex) + (stamp ? `-${stamp}` : ''));

// Each run gets a separate application database and module. Result names remain
// operator-facing and include the selected stack.
export const dbName = (track: Pick<TrackDefinition, 'slug'>, runIndex: number): string =>
  `app${track.slug ? `_${track.slug}` : ''}_run${runIndex}`;

export const moduleName = (track: Pick<TrackDefinition, 'slug'>, runIndex: number): string =>
  `app${track.slug ? `-${track.slug}` : ''}-run${runIndex}`;

export const resultsName = (track: TrackDefinition, backend: string, runIndex: number): string =>
  `${backend}${track.slug ? `-${track.slug}` : ''}-run${runIndex}`;

export function levelPrompt(track: TrackDefinition, level: number): string {
  const prefix = String(level).padStart(2, '0') + '-';
  const file = existsSync(track.prompts)
    ? readdirSync(track.prompts).find(f => f.startsWith(prefix))
    : null;
  if (!file) throw new Error(`No prompt for level ${level} in ${track.prompts}`);
  return readFileSync(join(track.prompts, file), 'utf8');
}

export function appendix(track: TrackDefinition, level: number): string {
  const f = join(track.contracts, `appendix-${String(level).padStart(2, '0')}.md`);
  return existsSync(f) ? readFileSync(f, 'utf8') : '';
}

// Re-run earlier guarantee suites so later levels cannot hide regressions.
export function suitesFor(track: TrackSuiteSource, level: number): TrackSuite[] {
  const at = (lvl: number): TrackSuite[] => (track.suites[String(lvl)] ?? [])
    .map(suite => ({ ...suite, spec: join(track.dir, suite.spec) }));
  if (!track.suites[String(level)]) {
    throw new Error(`No scenario suites declared for ${track.name} level ${level}`);
  }
  const declared = at(level);

  // Deduplicate inherited guarantees; the current declaration wins.
  const seen = new Set(declared.map(suite => suite.spec));
  const inherited: TrackSuite[] = [];
  for (let lvl = 1; lvl < level; lvl++) {
    for (const suite of at(lvl)) {
      if (suite.inherit !== 'all-higher-levels' || seen.has(suite.spec)) continue;
      seen.add(suite.spec);
      // Bundle keys must remain unique across inherited suites.
      inherited.push({ ...suite, id: `${suite.id}@L${lvl}`, inherited: true, fromLevel: lvl });
    }
  }
  return [...declared, ...inherited];
}
