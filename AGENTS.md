# Review tiers

A path's review tier comes from the nearest `.deep` marker in its directory or above it. Each marker is a symlink to `docs/DEEP_DATABASE_STYLE.md`.

- `.deep`: Deep. The deep core (datastore and indexes, commitlog, snapshots, durability) and the crates it builds on. This code follows the style doc or is moving toward it. Read the doc in full before your first edit here, and follow it.
- No marker: Core. A person reviews every line, so keep diffs small and focused.

`git ls-files '*/.deep'` lists every marker.

- A PR's tier is the highest tier among the files it touches, so keep Deep changes in their own PR.
- Deleting or weakening a test takes the tier of the code it covers, and the PR description must say so.
- A Deep crate must not depend on a Core crate (dev-dependencies are exempt). Some existing dependencies break this rule; do not add more.
- Do not add, remove, move or edit a marker unless asked.

These markers are provisional, and CI does not check any of this yet.
