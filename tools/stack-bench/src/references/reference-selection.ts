import { selectReferenceFixture } from './reference-fixtures.js';
import type { ReferenceFixture, ReferenceRegistry } from './reference-fixtures.js';
import { resolveRecipeRelease } from '../composition/recipe-release.js';
import type { RecipeBinding, RecipeRequest } from '../composition/recipe-release.js';
import { loadTrack } from '../composition/tracks.js';
import { validateConditionReference, type ConditionReference } from '../campaigns/condition-compiler.js';

export function parseReferenceCondition(value: string | undefined): ConditionReference | undefined {
  return value === undefined ? undefined : validateConditionReference(JSON.parse(value), 'reference condition');
}

export function assertReferenceAuthentication(
  fixtureId: string, requiredEnvironment: readonly string[], condition?: ConditionReference,
  qualification = false,
): void {
  if (requiredEnvironment.includes('OIDC_ISSUER') && condition?.authenticationProvider !== 'keycloak') {
    throw new Error(`${fixtureId} requires an explicitly selected study condition with authenticationProvider: keycloak; supply --condition-json. Provider availability is not enabled by reference metadata.`);
  }
  if (qualification && condition?.authenticationProvider) {
    throw new Error('Provider-enabled reference qualification is blocked: qualification scope, expected evidence validation, and reuse do not yet bind the study condition. Reference builds do not establish qualification.');
  }
}

export interface ReferenceSelectionArgs {
  backend: string;
  track: string;
  level: number;
  recipe?: RecipeRequest | null;
}

export interface ReferenceSelection {
  binding: RecipeBinding;
  fixture: ReferenceFixture;
  recipe: string;
}

export function resolveReferenceSelection(
  registry: ReferenceRegistry,
  args: ReferenceSelectionArgs,
): ReferenceSelection {
  const track = loadTrack(args.track);
  const binding = resolveRecipeRelease(track, args.level, args.recipe ?? null);
  if (!binding) throw new Error(`${args.track} L${args.level} has no recipe release`);
  const recipe = binding.release.id;
  const fixture = selectReferenceFixture(registry, { ...args, recipe });
  return { binding, fixture, recipe };
}
