import type { Track } from './tracks.mjs';
import type { CompiledRecipePlan } from './composition-compiler.mjs';

export interface RecipeCheck {
  stableKey: string;
  executionId: string;
  points: number;
  packId?: string;
  stablePackId?: string;
  packVersion?: string;
  checkGroupId?: string;
  role?: string;
  observations?: string[];
  requiresFeatures?: string[];
  source?: string;
  featureId?: number;
  criterionId?: string;
  description?: string;
  treatment?: string;
}

export interface RecipeTaskFragment {
  id: string;
  path: string;
  order: number;
  owners: string[];
  ownerConditions?: Array<{
    owner: string;
    modes: string[];
    requiresFeatures: string[];
  }>;
  modes: string[];
  requiresFeatures?: string[];
}

export interface RecipePackComponent {
  id: string;
  version: string;
  state: string;
  path: string;
  sha256: string;
  includeRoles: string[];
  stableId?: string;
  moduleType?: string;
  requiresPacks: string[];
}

export interface RecipeRelease {
  id: string;
  version: string;
  state: string;
  title: string;
  track: string;
  compatibility: { legacyLevel?: number; mode?: string } | null;
  meaningSha256: string;
  executionSha256: string;
  contentSha256: string;
  sourceManifestSha256: string;
  scoring: { mode: string; checks: number; points: number };
  checkCatalog: RecipeCheck[];
  components: {
    fixture: { id: string; version: string; state: string; path: string; sha256: string };
    packs: RecipePackComponent[];
  };
  task: {
    mode: string;
    baseRecipe: Pick<RecipeRelease, 'id' | 'version' | 'state' | 'track' | 'meaningSha256'
      | 'executionSha256' | 'contentSha256' | 'sourceManifestSha256'> | null;
    requirements: RecipeTaskFragment[];
    contracts: RecipeTaskFragment[];
    requirementSha256: string;
    contractSha256: string;
    composedSha256: string;
  };
}

export interface RecipeExecution {
  id: string;
  ownership: { kind: 'current' | 'inherited' };
}

export interface RecipeBinding {
  alias: string;
  status: string;
  catalog: unknown;
  plan: CompiledRecipePlan;
  release: RecipeRelease;
  execution: RecipeExecution[];
}

export function recipeReleaseIdentity(release: RecipeRelease): Pick<RecipeRelease,
  'id' | 'version' | 'state' | 'track' | 'meaningSha256' | 'executionSha256'
  | 'contentSha256' | 'sourceManifestSha256'>;

export function buildRecipeRelease(
  recipePath: string,
  options?: { trackRoot?: string },
): RecipeRelease;

export function resolveRecipeRelease(
  track: Track,
  level: number,
  requested?: string | { id: string; version: string; contentSha256?: string } | null,
): RecipeBinding;

export function recipeReleaseIdentity(release: RecipeRelease): unknown;
