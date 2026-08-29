export const TRACKS_DIR: string;
export const DEFAULT_TRACK: string;
export const RUN_INDEX_CAP: number;

export interface LoadedTrack {
  name: string;
  dir: string;
  contracts: string;
  scenarios: string;
  [key: string]: unknown;
}

export function listTracks(options?: { includeInternal?: boolean }): string[];

export interface Track extends LoadedTrack {
  schemaVersion: number;
  title: string;
  slug: string;
  internal: boolean;
  validatedThrough: number;
  plannedThrough: number;
  portOffset: number;
  restartProbe: string;
  reseedOnReset: boolean;
  reseedProbeExpectation: unknown;
  databaseProvenance: unknown;
  suites: Record<string, unknown[]>;
  actions: unknown[];
  prompts: string;
  walk: string;
}

export type TrackDefinition = Track;
export interface TrackSuite {
  id: string;
  spec: string;
  [key: string]: unknown;
}
export function loadTrack(name?: string): Track;
export function isDeclaredLevel(track: TrackDefinition, level: number): boolean;
export function suitesFor(track: TrackDefinition, level: number): TrackSuite[];
