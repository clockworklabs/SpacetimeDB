import type { Track } from './tracks.mjs';

export interface RecipeCheck {
  stableKey: string;
  executionId: string;
  points: number;
  packId?: string;
  source?: string;
  featureId?: string | null;
  criterionId?: string;
}

export interface RecipeRelease {
  id: string;
  version: string;
  state: string;
  track: string;
  meaningSha256: string;
  executionSha256: string;
  contentSha256: string;
  sourceManifestSha256: string;
  scoring: { mode: string; checks: number; points: number };
  checkCatalog: RecipeCheck[];
  components: { fixture: { sha256: string } };
  task: {
    mode: string;
    baseRecipe?: RecipeRelease;
  };
}

export interface RecipeExecution {
  id: string;
  ownership: { kind: 'current' | 'inherited' };
}

export interface RecipeBinding {
  alias: string;
  status: string;
  release: RecipeRelease;
  execution: RecipeExecution[];
}

export function buildRecipeRelease(
  recipePath: string,
  options?: { trackRoot?: string },
): RecipeRelease;

export function resolveRecipeRelease(
  track: Track,
  level: number,
  requested?: string | { id: string; version: string; contentSha256?: string } | null,
): RecipeBinding;
