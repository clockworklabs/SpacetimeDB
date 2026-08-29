export interface ResolvedFeatureCatalog {
  id: string;
  version: string;
  [key: string]: unknown;
}

export function resolveFeatureCatalog(requested: string, track: unknown): ResolvedFeatureCatalog;
