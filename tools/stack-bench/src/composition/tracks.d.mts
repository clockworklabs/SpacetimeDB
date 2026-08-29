export const TRACKS_DIR: string;
export const DEFAULT_TRACK: string;

export interface LoadedTrack {
  name: string;
  dir: string;
  contracts: string;
  scenarios: string;
  [key: string]: unknown;
}

export function listTracks(options?: { includeInternal?: boolean }): string[];
export type TrackDefinition = LoadedTrack;
export interface TrackSuite {
  id: string;
  spec: string;
  [key: string]: unknown;
}
export function loadTrack(name?: string): TrackDefinition;
export function suitesFor(track: TrackDefinition, level: number): TrackSuite[];
