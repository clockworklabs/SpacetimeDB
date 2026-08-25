# Authoring a SpacetimeDB TypeScript submodule

This guide defines the public conventions for packages in this repository.
A package may be a pure helper, a host-configured factory, or a mountable
submodule. Every entry point must match the package's implemented capabilities.

## Package shapes

- **Helper:** pure functions or typed dispatch. Examples include `crypto`,
  `agents`, and `cron`.
- **Factory:** creates tables and operations from host-supplied types or
  handlers. The host mounts the returned pieces into its schema.
- **Mountable submodule:** exports a reusable schema surface and an installation
  helper. The host owns initialization and route wiring.

Demo-specific tables, model names, task variants, and business rules belong in
`example/` or local build fixtures. Published `./submodule` exports contain the
reusable surface.

## Package setup

Use the scoped `@spacetimedb/<name>` package name and publish TypeScript source directly.
Declare `spacetimedb` as a peer dependency when the public API uses its types.

```json
{
  "name": "@spacetimedb/your-thing",
  "version": "0.1.0",
  "license": "BUSL-1.1",
  "type": "module",
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "exports": {
    ".": { "types": "./src/index.ts", "default": "./src/index.ts" }
  },
  "files": ["src", "LICENSE.txt", "README.md"],
  "publishConfig": {
    "access": "public",
    "registry": "https://registry.npmjs.org/"
  },
  "peerDependencies": { "spacetimedb": "workspace:^" },
  "devDependencies": { "spacetimedb": "workspace:*" }
}
```

Add subpath exports only when they are intentional public APIs. Every exported
path must be included in `files` and must work in the packed tarball.

Use pnpm workspace references for internal development. pnpm converts
`workspace:^` to a compatible release range when it packs a package. Pin the
CI CLI to the repository SDK version so release results are reproducible.

## Layout

```text
spacetime-your-thing-ts/
|-- src/              # published implementation and public types
|-- example/          # optional runnable integration
|   `-- spacetimedb/  # example host module
|-- spacetimedb/      # optional canonical module fixture
|-- scripts/          # package tests and release helpers
|-- package.json
|-- README.md
`-- LICENSE.txt
```

Use `spacetime-<name>-ts` for every top-level package directory. Use
`spacetimedb` for every host or fixture module directory. Publish a fixture only
when it is a documented, reusable entry point.

## Runtime conventions

1. External HTTP belongs in a procedure or HTTP handler. Reducers remain
   deterministic.
2. Use `ctx.timestamp` and context-provided randomness in module operations.
3. Store API keys and signing secrets in private tables. Routine public
   operations accept product data and return safe results.
4. Seed the publishing owner as the initial admin during `init`.
5. Keep per-user data private and expose it through caller-scoped views.
6. Bound scheduled and batch work. Preserve useful status or attempt history.
7. Treat outbound side effects as at-least-once unless the integration supplies
   and enforces an idempotency key.
8. Use `snake_case` for database table and operation names. Keep TypeScript
   identifiers readable and consistent with the surrounding package.

For scheduled-table forward references, capture the registered reducer and
validate the wiring during module definition.

## README requirements

Every package README uses these top-level sections in this order:

1. `Install`
2. `Usage`
3. `API`
4. `Limitations`
5. `Testing`
6. `License`

Start with one plain-language paragraph that states what the package does and
who owns persistence, authorization, and lifecycle wiring. Examples must be
syntactically valid and use current exports. Include performance numbers,
provider claims, and platform statements only when the repository verifies and
maintains them.

Under `Usage`, include an `Integrate into an application` subsection. It must
identify the package as a helper, factory, or mountable submodule and show the
smallest complete host integration. Avoid unexplained identifiers in the first
snippet. If application-specific functions are unavoidable, label the snippet
as a skeleton and name every placeholder. Link full examples with repository
URLs that continue to work when npm renders the packed README.

Example READMEs must include `Prerequisites`, `Quick start`, and `Use in your
project`. State that checked-in workspace dependencies are for repository
development and show the published npm install command. Environment-file setup
must use one cross-platform command or show both Bash and PowerShell forms.

## Release checks

Before publishing:

```bash
pnpm install
pnpm components:check
pnpm components:build
```

The root lint command also validates Markdown links, TypeScript/JavaScript code
fences, stale work-in-progress markers, and the required README structure.
Follow [`NPM_RELEASE_CHECKLIST.md`](./NPM_RELEASE_CHECKLIST.md) for versioning,
npm authentication, publication order, and post-publish verification.
