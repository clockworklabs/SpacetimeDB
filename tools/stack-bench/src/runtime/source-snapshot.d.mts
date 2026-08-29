export interface SourceSnapshot {
  sha256: string;
  files: number;
  bytes: number;
}

export function hashAppSource(appDir: string): SourceSnapshot;
export function restoreAppSource(from: string, appDir: string): void;
