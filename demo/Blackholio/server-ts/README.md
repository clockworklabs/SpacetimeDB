# Blackholio TypeScript Server

SpacetimeDB TypeScript implementation of the Blackholio server module. This
module is intended to match `../server-rust` so the browser client can target
either server after regenerating bindings.

This is a standalone pnpm project. Its local `pnpm-workspace.yaml` prevents pnpm
from selecting the repository workspace, and both pnpm config files enforce a
1440-minute minimum release age.

The checked-in dependency configuration links the TypeScript SDK from this
repository during development:

```json
"spacetimedb": "link:../../../crates/bindings-typescript"
```

When publishing this demo outside the SpacetimeDB repository, replace the local
link with the current published npm package version.

## Commands

```bash
pnpm install
pnpm run typecheck
spacetime build
./generate.sh
./publish.sh
```

If `pnpm install` is interrupted while fetching packages, treat the generated
`node_modules` directory as disposable and rerun the install when the registry is
available. Do not work around minimum release age with `resolution-mode`.
