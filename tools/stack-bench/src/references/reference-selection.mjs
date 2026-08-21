import { selectReferenceFixture } from './reference-fixtures.mjs';
import { resolveRecipeRelease } from '../composition/recipe-release.mjs';
import { loadTrack } from '../composition/tracks.mjs';

export function resolveReferenceSelection(registry, args) {
  const track = loadTrack(args.track);
  const binding = resolveRecipeRelease(track, args.level, args.recipe ?? null);
  if (!binding) throw new Error(`${args.track} L${args.level} has no recipe release`);
  const recipe = `${binding.release.id}@${binding.release.version}`;
  const fixture = selectReferenceFixture(registry, { ...args, recipe });
  return { binding, fixture, recipe };
}
