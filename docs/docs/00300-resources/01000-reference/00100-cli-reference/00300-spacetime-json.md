---
title: spacetime.json
slug: /cli-reference/spacetime-json
---

# `spacetime.json` Configuration File

The `spacetime.json` file defines project-level configuration for the SpacetimeDB CLI. It eliminates repetitive CLI flags and enables multi-target workflows such as publishing multiple databases or generating bindings for multiple languages from a single project.

Commands that read `spacetime.json` include [`spacetime publish`](/cli-reference#spacetime-publish), [`spacetime generate`](/cli-reference#spacetime-generate), and [`spacetime dev`](/cli-reference#spacetime-dev).

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
| `generate` | array | No | Generate targets (see [Generate configuration](#generate-configuration)) |
| `children` | array | No | Child database entities (see [Children and inheritance](#children-and-inheritance)) |
| `dev` | object | No | Dev server configuration, root-level only (see [`spacetime dev` configuration](#spacetime-dev-configuration)) |

\* `module-path`, `bin-path`, and `js-path` are mutually exclusive. If a child specifies any one of these, the other two are not inherited from the parent. See [Source conflict rule](#source-conflict-rule).

## Generate configuration

The `generate` key is an array of objects. Each object configures bindings generation for a specific language and output location.

| Field | Type | Description |
|-------|------|-------------|
| `language` | string | Target language: `typescript`, `csharp`, `rust`, `unrealcpp` (required) |
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

## Children and inheritance

The `children` array defines additional database targets. Each child inherits most fields from the root by default.

### What children inherit

All fields listed in the [Fields reference](#fields-reference) with "Yes" in the Inherited column are inherited. A child can override any inherited field by specifying it explicitly.

The following fields are never inherited:

- `database`: each child must define its own.
- `generate`: tied to a specific module and output location. Inheriting generate targets is redundant when the child shares the parent's module (deduplication already handles it) and dangerous when the child uses a different module (two modules would write bindings into the same output directory).
- `children`: structural, not a database property.
- `dev`: root-level only.

### Source conflict rule

`module-path`, `bin-path`, and `js-path` are mutually exclusive module sources (mirroring the CLI's existing conflict group for `--module-path`, `--bin-path`, and `--js-path`). If a child specifies any one of these, the other two are not inherited from the parent. This prevents a child from accidentally inheriting a `bin-path` that points to a different module's precompiled binary.

### Multi-database example

```json
{
  "database": "region-us",
  "module-path": "./region-module",
  "server": "testnet",
  "build-options": "--release",
  "generate": [
    { "language": "typescript", "out-dir": "./client/src/bindings" }
  ],
  "children": [
    { "database": "region-eu" },
    { "database": "region-asia" }
  ]
}
```

All three databases (`region-us`, `region-eu`, `region-asia`) share the same module, server, and build options via inheritance. Because all three databases use the same module and generate config, bindings are generated only once.

### Different modules

A child can override `module-path` to use a different module:

```json
{
  "database": "my-game-global",
  "module-path": "./global-module",
  "server": "testnet",
  "generate": [
    { "language": "typescript", "out-dir": "./web/src/global-bindings" },
    { "language": "csharp", "out-dir": "./unity/Assets/GlobalBindings" }
  ],
  "children": [
    {
      "database": "my-game-region",
      "module-path": "./region-module",
      "generate": [
        { "language": "typescript", "out-dir": "./web/src/region-bindings" }
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

1. Build and publish all databases defined in the config.
2. Generate bindings for all databases.
3. Run the client dev server specified by `dev.run`.
4. Watch for changes and repeat steps 1-3.

The `--server` flag overrides the server for all databases. The `--skip-publish` flag skips step 1, and `--skip-generate` skips step 2.

### Config auto-generation

If no config file exists, `spacetime dev` generates a `spacetime.dev.json` with a minimal config. The command prompts for the database name (pre-filled with the directory name as the default) and infers the client language and output directory from the project structure. Values that the CLI already defaults (such as `module-path` to `./spacetimedb`) are omitted.

### Safety prompt

`spacetime dev` tracks which config file the publish targets were resolved from. If the publish configuration comes from `spacetime.json` or `spacetime.local.json` (rather than a dev-specific file like `spacetime.dev.json` or `spacetime.dev.local.json`), the command prompts for confirmation before publishing. This prevents accidentally publishing to a shared or production server during development.

## Database selection

When `spacetime.json` exists, the database name positional argument selects which databases to operate on. Glob patterns are supported.

```bash
# Operate on all databases
spacetime publish

# Operate on a specific database
spacetime publish region-us

# Operate on databases matching a pattern
spacetime publish "region-*"
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
- `--build-options`: build flags
- `--break-clients`: allow breaking changes
- `--delete-data`: clear database data
- `--yes` / `--force`: skip confirmation prompts

### Per-database overrides

These produce an error if multiple databases are selected:

- `--module-path`: module source path
- `--bin-path`: precompiled WASM path
- `--js-path`: JS bundle path
- `--num-replicas`: replica count

### Per-generate-entry overrides

These produce an error if the selected database has multiple generate entries:

- `--lang`: target language
- `--out-dir`: output directory
- `--namespace`: C# namespace
- `--unreal-module-name`: Unreal module name
- `--uproject-dir`: Unreal project directory

## `--no-config`

The `--no-config` flag causes the CLI to ignore `spacetime.json` entirely, behaving as if no config file exists. This is useful for one-off operations or scripting.

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
3. `spacetime.local.json`
4. `spacetime.{env}.json`
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
