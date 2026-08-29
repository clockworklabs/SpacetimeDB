import { selectReferenceFixture } from './reference-fixtures.mjs';
import type { ReferenceFixture, ReferenceRegistry } from './reference-fixtures.mjs';
import { resolveRecipeRelease } from '../composition/recipe-release.mjs';
import type { RecipeBinding } from '../composition/recipe-release.mjs';
import { loadTrack } from '../composition/tracks.mjs';

export interface ReferenceSelectionArgs {
  backend: string;
  track: string;
  level: number;
  recipe?: string | { id: string; version: string; contentSha256?: string } | null;
  [key: string]: unknown;
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
  const recipe = `${binding.release.id}@${binding.release.version}`;
  const fixture = selectReferenceFixture(registry, { ...args, recipe });
  return { binding, fixture, recipe };
}
