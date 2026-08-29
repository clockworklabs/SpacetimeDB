import type { CompiledRecipePlan } from './composition-compiler.js';
import type { RecipeCheck, RecipeRelease } from './recipe-release.js';

export interface RecipeSelection {
  schemaVersion: number;
  recipe: { id: string; version: string; contentSha256: string };
  requested: { packs: string[]; checks: string[] };
  taskPacks: string[];
  sha256: string;
  completeness: 'full' | 'subset';
  scoredPoints: number;
  checks: RecipeCheck[];
  features?: string[];
  promptPacks?: string[];
  taskSelectionSha256?: string;
}

export interface RecipeSelectionOptions {
  packIds?: string[];
  checkKeys?: string[];
}

export interface SelectedRecipeRelease extends RecipeRelease {
  selection: RecipeSelection;
}

export interface ComposedRecipeTask {
  schemaVersion: number;
  recipeContentSha256: string;
  selectionSha256: string;
  taskMode?: string;
  requirementIds: string[];
  contractIds: string[];
  requirementSha256: string;
  contractSha256: string;
  sha256: string;
  requirementText: string;
  contractText: string;
}

export interface RecipeTaskRequestResult {
  request: {
    schemaVersion: number;
    task: { mode?: string };
    [key: string]: unknown;
  };
  selection: RecipeSelection & {
    requested: Record<string, unknown>;
    promptPacks?: string[];
    features?: string[];
    specifications?: { requested: string[]; expected: string[]; observed: string[] };
    scoredChecks: RecipeCheck[];
    observedChecks: RecipeCheck[];
  };
  task: ComposedRecipeTask;
}

export function createRecipeTaskRequest(binding: import('./recipe-release.js').RecipeBinding,
  options?: RecipeSelectionOptions): RecipeTaskRequestResult;
export function createBoundRecipeTaskRequest(binding: import('./recipe-release.js').RecipeBinding,
  options?: RecipeSelectionOptions & {
    featureIds?: string[];
    requestedSpecifications?: string[];
    expectedSpecifications?: string[];
    observedSpecifications?: string[];
    dependencyExpansion?: string;
    taskMode?: string;
  }): RecipeTaskRequestResult;

export function resolveRecipeSelection(
  release: RecipeRelease,
  options?: RecipeSelectionOptions,
): RecipeSelection;

export function selectRecipeRelease(
  release: RecipeRelease,
  options?: RecipeSelectionOptions,
): SelectedRecipeRelease;

export function composeSelectedRecipeTask(
  plan: CompiledRecipePlan,
  selection: RecipeSelection,
  options?: { taskMode?: string | null },
): ComposedRecipeTask;

export function createBoundRecipeTaskRequest(binding: unknown, options?: unknown): unknown;
export function createAgentVisibleTaskRequest(binding: unknown, selected: unknown): unknown;
