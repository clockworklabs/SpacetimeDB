# Development

Maintainer notes for the SpacetimeDB plugin. User facing docs live in `README.md`.

## The skills are a copy, not a symlink

The payload at `plugins/spacetimedb/skills/` is a byte for byte copy of the repository's `skills/` directory. A symlink does not work: plugin installers materialize the payload without following symlinks, so a symlinked skills directory installs empty. The copy also keeps the payload self contained, which plugin distribution requires.

## Keeping the copy in sync

After changing anything under `skills/`:

```bash
node codex-plugin/scripts/check-skills-sync.ts --fix   # resync the copy
node codex-plugin/scripts/check-skills-sync.ts         # verify, exits 1 on drift
```

The check compares every file byte for byte in both directions, so it catches edits, missing files, and extraneous files. CI runs the same comparison in `cargo ci lint`, so drift fails the lint job. The script runs directly on Node 22.18+ with no install. To type check it, run from the repository root:

```bash
pnpm install --filter spacetimedb-plugin-scripts
pnpm --filter spacetimedb-plugin-scripts typecheck
```

## Releasing a change

1. Sync the skills copy (above).
2. Bump `version` in `.codex-plugin/plugin.json`. Installs are cached by version, so an unbumped version serves stale content to anyone who reinstalls.
3. Keep the two Codex marketplace catalogs (`.agents/plugins/marketplace.json` here and at the repository root) identical apart from `source.path`, which must never point at `./`.
4. Keep `.mcp.json` in the wrapped `mcpServers` form. Published Codex plugins use that shape.
