import { selectReferenceFixture } from './reference-fixtures.js';
import type { ReferenceFixture, ReferenceRegistry } from './reference-fixtures.js';
import { resolveRecipeRelease } from '../composition/recipe-release.js';
import type { RecipeBinding, RecipeRequest } from '../composition/recipe-release.js';
import { loadTrack } from '../composition/tracks.js';

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
