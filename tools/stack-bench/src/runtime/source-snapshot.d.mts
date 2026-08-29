export interface SourceSnapshot {
  sha256: string;
  files: string[];
}

export function hashAppSource(appDir: string): SourceSnapshot;
export function restoreAppSource(from: string, appDir: string): void;
export function snapshotAppSource(appDir: string, to: string): void;
