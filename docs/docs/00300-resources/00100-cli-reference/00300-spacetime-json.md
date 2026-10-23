---
title: spacetime.json
slug: /cli-reference/spacetime-json
---

# `spacetime.json` Configuration File

The `spacetime.json` file defines project-level configuration for the SpacetimeDB CLI. It eliminates repetitive CLI flags and enables multi-target workflows such as publishing multiple databases or generating bindings for multiple languages from a single project.

Commands that read `spacetime.json` include [`spacetime publish`](/cli-reference#spacetime-publish), [`spacetime generate`](/cli-reference#spacetime-generate), and [`spacetime dev`](/cli-reference#spacetime-dev). Commands that operate on an already-published database, such as [`spacetime call`](/cli-reference#spacetime-call), [`spacetime sql`](/cli-reference#spacetime-sql), and [`spacetime logs`](/cli-reference#spacetime-logs), can resolve their database and server from matching config targets. Commands that operate on a server rather than a named config database, such as [`spacetime list`](/cli-reference#spacetime-list), [`spacetime rename`](/cli-reference#spacetime-rename), and [`spacetime mcp`](/cli-reference#spacetime-mcp), can also use the configured server when the config has a single unambiguous server.

## Config structure

The config is database-centric. The top-level object represents the root database, which is always required. The root database may have `children` that define additional database targets, each inheriting the root's settings by default.

```json
{
  "database": "my-game",
  "module-path": "./server",
  "dev": { "run": "pnpm dev" },
  "generate": [
    { "language": "typescript", "out-dir": "./client/src/bindings" }
  ]
}
```

## Fields reference

These fields can appear at any level (root or child):

| Field | Type | Inherited | Description |
|-------|------|-----------|-------------|
| `database` | string | No | Database name or identity (required) |
| `module-path` | string | Yes\* | Path to module source directory |
| `bin-path` | string | Yes\* | Path to precompiled WASM binary |
| `js-path` | string | Yes\* | Path to bundled JavaScript file |
| `server` | string | Yes | Server nickname, domain, or URL |
| `build-options` | string | Yes | Options passed to the build command |
| `break-clients` | boolean | Yes | Allow breaking changes |
| `num-replicas` | number | Yes | Number of database replicas |
| `anonymous` | boolean | Yes | Use anonymous identity |
| `organization` | string | Yes | Organization name or identity |
| `dependencies` | array | No | Receiver binding inputs for this calling database (see [Receiver dependencies](#receiver-dependencies)) |
| `generate` | array | No | Generate targets (see [Generate configuration](#generate-configuration)) |
| `children` | array | No | Child database entities (see [Children and inheritance](#children-and-inheritance)) |
| `dev` | object | No | Dev server configuration, root-level only (see [`spacetime dev` configuration](#spacetime-dev-configuration)) |

\* `module-path`, `bin-path`, and `js-path` are mutually exclusive. If a child specifies any one of these, the other two are not inherited from the parent. See [Source conflict rule](#source-conflict-rule).

## Generate configuration

The `generate` key is an array of objects. Each object configures bindings generation for a specific language and output location.

| Field | Type | Description |
|-------|------|-------------|
| `language` | string | Target language: `typescript`, `csharp`, `rust`, `unrealcpp`; `cpp` is reserved for module bindings |
| `bindings` | string | `client` (default) for application clients, or `module` for inter-database calls |
| `out-dir` | string | Output directory for generated files |
| `namespace` | string | C# namespace (`csharp` only) |
| `unreal-module-name` | string | Unreal module name (`unrealcpp` only) |
| `uproject-dir` | string | Unreal project directory (`unrealcpp` only) |
| `include-private` | boolean | Include private tables in generated code |

Generate entries use the `module-path` (or `bin-path`/`js-path`) from their parent entity to determine which module to build and generate from.

When `spacetime generate` runs, it deduplicates by module path. If multiple databases share the same module and generate config (for example, via inheritance), bindings are generated once.

### Example

```json
{
  "database": "my-game",
  "module-path": "./server",
  "generate": [
    { "language": "typescript", "out-dir": "./web/src/bindings" },
    {
      "language": "csharp",
      "out-dir": "./unity/Assets/Bindings",
      "namespace": "MyGame.Bindings"
    }
  ]
}
```

## Receiver dependencies

`spacetime generate` produces two kinds of bindings:

- **Client bindings** are used by clients to connect to a database, subscribe to data, and call its reducers.
- **Module bindings**, also called **receiver bindings**, are used inside one database module to make asynchronous inter-database communication (IDC) calls to another database's reducers. Rust and TypeScript modules support these bindings; C# and C++ module bindings are not implemented yet.

With `spacetime.json`, regular `generate` entries default to `"bindings": "client"`; set `"bindings": "module"` in an entry to select module bindings explicitly. Entries synthesized from `dependencies` automatically use module bindings, so no `--bindings` CLI flag is needed for receiver dependencies.

The **calling database** contains the code making the inter-database call. The **receiving database** contains the reducers being called. In the examples below, `lobby` is the calling database and `game-world` is the receiving database. Declare receiver binding inputs in the calling database's `dependencies` array.

Each calling database with dependencies must define `module-path`. Dependencies are not inherited by children.

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Local dependency name passed to receiver binding generation; not the receiver's database name |
| `module-path` | One source required | Receiver source directory; takes precedence over `database` when both are set |
| `database` | One source required | Exact name of another target in the same config, used to look up its local module source; not a live-schema fetch |
| `out-dir` | No | Output directory, relative to the config directory |
| `build-options` | No | Receiver build options when using a direct `module-path` |
| `dotnet-version` | No | Receiver .NET SDK version when using a direct `module-path` |

### Receiver source without a publish target

A direct receiver source does not need its own database target:

```json
{
  "database": "lobby",
  "module-path": "./lobby",
  "dependencies": [
    {
      "name": "game_world",
      "module-path": "./game_world",
      "out-dir": "./lobby/src/remote_bindings"
    }
  ]
}
```

Declaring a dependency does not publish the receiver. To make it a publish target too, define it under `children` and reference its `database`:

```json
{
  "database": "lobby",
  "module-path": "./lobby",
  "dependencies": [
    { "name": "game_world", "database": "game-world" }
  ],
  "children": [
    { "database": "game-world", "module-path": "./game_world" }
  ]
}
```

A `database` reference uses the target's `module-path`, `bin-path`, or `js-path`, and its resolved receiver build options and .NET SDK version. It fails if no matching target or module source exists. A dependency does not inherit the calling module's build options or .NET SDK version.

`spacetime generate lobby` selects the calling database's regular `generate` entries and dependencies; without a database filter, generation includes all targets' entries. The module binding language is inferred from the calling module, not the external client project. Rust and TypeScript module bindings default to `<calling module-path>/src/remote_bindings`.

### Explicit module-binding request

Without configuration, select the binding kind with `--bindings client` (the default) or `--bindings module`. When using `spacetime.json`, `--bindings` can override the binding kind only if a single generation entry is selected; it is rejected when multiple entries are selected. Prefer the config fields described above for configured generation. Here `--module-path` identifies the **receiving module**, while `--out-dir` explicitly places output in the **calling module** project:

```bash
spacetime generate --no-config --bindings module --lang rust \
  --module-path ./game_world --out-dir ./lobby/src/remote_bindings
```

This generates Rust module bindings for `game_world` in the `lobby` module's source tree. Use `--lang typescript` for a TypeScript calling module. `unrealcpp` is client-only; the CLI accepts `cpp` only with `--bindings module`, but C++ module binding generation is not implemented yet.

## Children and inheritance

The `children` array defines additional database targets. Each child inherits most fields from the root by default.

### What children inherit

All fields listed in the [Fields reference](#fields-reference) with "Yes" in the Inherited column are inherited. A child can override any inherited field by specifying it explicitly.

The following fields are never inherited:

- `database`: each child must define its own.
- `dependencies`: receiver binding inputs belong to each calling database.
- `generate`: tied to a specific module and output location. Inheriting generate targets is redundant when the child shares the parent's module (deduplication already handles it) and dangerous when the child uses a different module (two modules would write bindings into the same output directory).
- `children`: structural, not a database property.
- `dev`: root-level only.

### Source conflict rule

`module-path`, `bin-path`, and `js-path` are mutually exclusive module sources (mirroring the CLI's existing conflict group for `--module-path`, `--bin-path`, and `--js-path`). If a child specifies any one of these, the other two are not inherited from the parent. This prevents a child from accidentally inheriting a `bin-path` that points to a different module's precompiled binary.

### Multi-database example

```json
{
  "database": "world-highlands",
  "module-path": "./world-module",
  "server": "maincloud",
  "build-options": "--release",
  "generate": [
    { "language": "typescript", "out-dir": "./client/src/bindings" }
  ],
  "children": [
    { "database": "world-midlands" },
    { "database": "world-coastlands" }
  ]
}
```

All three databases (`world-highlands`, `world-midlands`, `world-coastlands`) share the same module, server, and build options via inheritance. Because all three databases use the same module and generate config, bindings are generated only once.

### Different modules

A child can override `module-path` to use a different module:

```json
{
  "database": "my-game-global",
  "module-path": "./global-module",
  "server": "maincloud",
  "generate": [
    { "language": "typescript", "out-dir": "./web/src/global-bindings" },
    { "language": "csharp", "out-dir": "./unity/Assets/GlobalBindings" }
  ],
  "children": [
    {
      "database": "my-game-world",
      "module-path": "./world-module",
      "generate": [
        { "language": "typescript", "out-dir": "./web/src/world-bindings" }
      ]
    }
  ]
}
```

The child overrides `module-path` and `generate`, while inheriting `server` from the root.

## `spacetime dev` configuration

The `dev` key is a root-level-only setting that specifies the client development server command:

```json
{
  "dev": { "run": "pnpm dev" }
}
```

The `--run` CLI flag overrides `dev.run`.

### Behavior

When running `spacetime dev`:

1. Build all modules defined in the config.
2. Generate bindings for all databases.
3. Publish all databases.
4. Run the client dev server specified by `dev.run`.
5. Watch for changes and repeat steps 1-4.

The `--server` flag overrides the server for all databases. The `--skip-publish` flag skips step 3, and `--skip-generate` skips step 2.

#### Additional behavior with receiver dependencies

If the root database declares `dependencies`, `dev` generates its receiver bindings before building that database's module, both on startup and on rebuilds. This adds a pre-build step without changing the normal development workflow above.

Receiver dependency sources are watched even when they are outside the calling module directory or not selected for publishing. Child targets can supply receiving-module sources for lookup, but `dev` does not process their own `dependencies` arrays.

`--skip-generate` skips both receiver and client binding generation and excludes receiver-only sources from watching. It does not create any bindings the calling module needs to compile. `--skip-publish` skips publishing, not binding generation.

### Config auto-generation

If no config file exists, `spacetime dev` generates a `spacetime.json` with the server and module path, and a `spacetime.local.json` with the database name. The command prompts for the database name (pre-filled with the directory name as the default).

### Safety prompt

`spacetime dev` tracks which config file the publish targets were resolved from. If the database is defined in `spacetime.json` (which is checked into git and typically defines shared or production databases), the command prompts for confirmation before publishing. Databases defined in dev-specific or local config files (such as `spacetime.dev.json` or `spacetime.local.json`) do not trigger this prompt.

## Database selection

When `spacetime.json` exists, the database name positional argument selects which databases to operate on. Glob patterns are supported.

```bash
# Operate on all databases
spacetime publish

# Operate on a specific database
spacetime publish world-highlands

# Operate on databases matching a pattern
spacetime publish "world-*"
```

When no database name is provided, all databases are selected. When a filter matches no databases, the CLI reports an error:

```
$ spacetime publish typo-name
Error: No database 'typo-name' found in spacetime.json.
Use --no-config to ignore the config file.
```

## Flag overrides

All CLI flags besides the database selector act as overrides. They are classified as global, per-database, or per-generate-entry:

### Global overrides

These apply to all selected databases:

- `--server`: target server
- `--break-clients`: allow breaking changes
- `--delete-data=<mode>`: clear database data (`always`, `on-conflict`, or `never`)
- `--yes`: skip confirmation prompts

### Per-database overrides

These produce an error if multiple databases are selected:

- `--module-path`: module source path
- `--bin-path`: precompiled WASM path
- `--js-path`: JS bundle path
- `--build-options`: build flags
- `--num-replicas`: replica count

### Per-generate-entry overrides

These produce an error if the selected database has multiple generate entries:

- `--lang`: target language
- `--out-dir`: output directory
- `--namespace`: C# namespace
- `--unreal-module-name`: Unreal module name
- `--uproject-dir`: Unreal project directory

## `--no-config`

The `--no-config` flag causes the CLI to ignore `spacetime.json` entirely, behaving as if no config file exists. This is useful for one-off operations, scripting, or commands such as `spacetime list` when you want the default server instead of the server configured for the current project.

```bash
spacetime publish my-db --module-path ./module --no-config
```

## `--env` and environments

Config files support environment and local overrides via a naming convention:

| File | Purpose | Checked in to git? |
|------|---------|---------------------|
| `spacetime.json` | Project defaults | Yes |
| `spacetime.{env}.json` | Environment-specific config | Yes |
| `spacetime.local.json` | User-specific overrides | No |
| `spacetime.{env}.local.json` | User + environment-specific overrides | No |

The *env* placeholder is set via the `--env` CLI flag (for example, `--env dev`). The `spacetime dev` command implicitly uses `--env dev`.

When `--env` is not specified, `spacetime publish` and `spacetime generate` load only the base config files (`spacetime.json` and `spacetime.local.json`).

### Precedence

From highest to lowest priority:

1. CLI flags (highest)
2. `spacetime.{env}.local.json`
3. `spacetime.{env}.json`
4. `spacetime.local.json`
5. `spacetime.json`
6. Built-in defaults (lowest)

CLI flags only override config values when explicitly provided by the user. Default flag values do not override config file values.

### Override behavior

Higher-priority files replace whole keys, not merge them. For example:

```json
// spacetime.json (checked in, shared project defaults)
{
  "database": "my-game",
  "module-path": "./server",
  "server": "maincloud",
  "generate": [
    { "language": "typescript", "out-dir": "./client/src/bindings" },
    { "language": "csharp", "out-dir": "./unity/Assets/Bindings" }
  ]
}
```

```json
// spacetime.dev.json (checked in, development environment)
{
  "server": "local",
  "database": "my-game-dev"
}
```

The result when running with `--env dev` (or via `spacetime dev`, which implies `--env dev`):

```json
{
  "database": "my-game-dev",
  "module-path": "./server",
  "server": "local",
  "generate": [
    { "language": "typescript", "out-dir": "./client/src/bindings" },
    { "language": "csharp", "out-dir": "./unity/Assets/Bindings" }
  ]
}
```

The `database` and `server` fields come from `spacetime.dev.json`. The `module-path` and `generate` fields are inherited from the base `spacetime.json`.

A developer can further override with a local file:

```json
// spacetime.dev.local.json (NOT checked in, personal overrides)
{
  "database": "my-game-dev-tyler"
}
```

This gives each developer their own database name for development without conflicts, while sharing the rest of the dev configuration.

## Config file discovery

The CLI searches for `spacetime.json` starting from the current directory and walking up the directory tree until it finds one or reaches the filesystem root. All paths in the config are relative to the directory containing `spacetime.json`.

## Editor support

The config file uses JSON5 syntax, which supports comments and trailing commas. The file uses a `.json` extension for broad editor compatibility.

Editors that enforce strict JSON on `.json` files will flag comments as errors. In VSCode, this can be resolved by adding to your settings:

```json
{
  "files.associations": { "spacetime.json": "jsonc" }
}
```
