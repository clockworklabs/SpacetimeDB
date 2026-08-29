export const TRACKS_DIR: string;
export function listTracks(options?: { includeInternal?: boolean }): string[];
export interface TrackDefinition {
  dir: string;
  [key: string]: unknown;
}
export interface TrackSuite {
  id: string;
  spec: string;
  [key: string]: unknown;
}
export function loadTrack(name?: string): TrackDefinition;
export function suitesFor(track: TrackDefinition, level: number): TrackSuite[];
