import type { CompiledFeature } from './definition-compiler.mjs';

export interface TaskFragmentDefinition {
  id: string;
  path: string;
  order: number;
  from?: string;
  until?: string;
  modes?: string[];
  requiresFeatures?: string[];
}

export interface CompiledTaskFragment
  extends Omit<TaskFragmentDefinition, 'from' | 'until' | 'modes'> {
  from: string | null;
  until: string | null;
  modes: string[];
  text: string;
}

export interface CompiledOwnedTaskFragment extends CompiledTaskFragment {
  owners: string[];
  ownerConditions?: Array<{
    owner: string;
    modes: string[];
    requiresFeatures: string[];
  }>;
  requiresFeatures?: string[];
}

export interface PackCheck {
  id: string;
  stableId?: string;
  source: string;
  feature: number;
  criteria?: string[];
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
}

export interface CompiledPackDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  stableId?: string;
  version: string;
  state: string;
  title: string;
  moduleType?: string;
  requiresPacks: string[];
  conflictsWith: string[];
  capabilities: string[];
  evidence: string[];
  budget: { status: string; maxRuntimeMs?: number };
  task: { requirements: TaskFragmentDefinition[]; contracts: TaskFragmentDefinition[] };
  checks: PackCheck[];
}

export interface FixtureItem {
  name: string;
  price: string;
  category: string;
  stock: Record<string, number>;
}

export interface FixtureAccount {
  username: string;
  password: string;
  roles: string[];
}

export interface CompiledFixtureDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  warehouses: string[];
  items: FixtureItem[];
  accounts: FixtureAccount[];
  empty: string[];
}

export interface SelectedCheckGroup {
  packId: string;
  packVersion: string;
  stablePackId?: string;
  moduleType?: string;
  checkGroupId: string;
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
  source: string;
  feature: CompiledFeature;
  actions: string[];
}

export interface SelectedCheck {
  stableKey: string;
  packId: string;
  stablePackId?: string;
  checkGroupId: string;
  criterionId: string;
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
  source: string;
  featureId: number;
  description: string;
  sourcePoints: number;
  points: number;
}

export interface CompiledRecipePlan {
  compositionSchemaVersion: number;
  recipe: {
    id: string;
    version: string;
    state: string;
    title: string;
    track: string;
    compatibility: { legacyLevel?: number; mode?: string } | null;
    task: {
      mode: string;
      baseRecipe: { id: string; version: string; path: string } | null;
      requirements: CompiledOwnedTaskFragment[];
      contracts: CompiledOwnedTaskFragment[];
      requirementText: string;
      contractText: string;
    };
  };
  fixture: CompiledFixtureDefinition;
  packs: Array<{
    id: string;
    stableId?: string;
    version: string;
    state: string;
    title: string;
    moduleType?: string;
    path: string;
    includeRoles: string[];
    requiresPacks: string[];
    capabilities: string[];
    evidence: string[];
    budget: { status: string; maxRuntimeMs?: number };
    task: { requirementIds: string[]; contractIds: string[] };
    actions: string[];
  }>;
  capabilities: string[];
  execution: Array<{ id: string; source: string; checkGroups: SelectedCheckGroup[] }>;
  checks: SelectedCheck[];
  scoring: { mode: string; checks: number; points: number };
}

export interface PromotionEntry {
  alias: string;
  status: string;
  recipe: { id: string; version: string; path: string };
}

export interface CompiledPromotionDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  entries: PromotionEntry[];
}

export interface CompiledPromotionCatalog {
  compositionSchemaVersion: number;
  catalog: { id: string; version: string; state: string; title: string };
  entries: PromotionEntry[];
}

export const COMPOSITION_SCHEMA_VERSION: number;

export function resolveTaskFragment(
  input: unknown,
  options: { trackRoot: string; source?: string; sourceCache?: Map<string, string> },
): CompiledTaskFragment;
export function compilePackDefinition(
  input: unknown,
  options?: { source?: string },
): CompiledPackDefinition;
export function compileFixtureDefinition(
  input: unknown,
  options?: { source?: string },
): CompiledFixtureDefinition;
export function compileRecipeDefinition(input: unknown, options?: { source?: string }): Record<string, unknown>;
export function compileRecipeFile(
  path: string,
  options?: { trackRoot?: string; availableCapabilities?: string[] | null; recipeStack?: string[] },
): CompiledRecipePlan;
export function compilePromotionDefinition(
  input: unknown,
  options?: { source?: string },
): CompiledPromotionDefinition;
export function compilePromotionFile(
  path: string,
  options?: { trackRoot?: string },
): CompiledPromotionCatalog;

export type CompiledRecipeRelease = CompiledRecipePlan;
