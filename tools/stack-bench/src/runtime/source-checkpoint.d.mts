export interface LevelCheckpoint {
  artifact: string;
  directory: string;
  sha256: string;
  files: number;
}

export function preserveLevelCheckpoint(input: {
  appDir: string;
  outputDir: string;
  runId: string;
  identities: unknown;
  track: string;
  backend: string;
  level: number;
  repair: unknown;
  outcome: unknown;
  selectionSha256?: string | null;
}): LevelCheckpoint;
