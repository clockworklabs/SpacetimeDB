# SpacetimeDB's cargo ci

## Overview

This document provides an overview of the `cargo ci` command-line tool, and documentation for each of its subcommands and options.

## `cargo ci`

SpacetimeDB CI tasks

This tool provides several subcommands for automating CI workflows in SpacetimeDB.

It may be invoked via `cargo ci <subcommand>`, or simply `cargo ci` to run all subcommands in sequence. It is mostly designed to be run in CI environments via the github workflows, but can also be run locally

**Usage:**
```bash
Usage: cargo ci [OPTIONS] [COMMAND]
```

**Options:**

- `--skip`: Skip specified subcommands when running all

When no subcommand is specified, all subcommands are run in sequence. This option allows specifying subcommands to skip when running all. For example, to skip the `unreal-tests` subcommand, use `--skip unreal-tests`.

- `--help`: Print help (see a summary with '-h')

### `test`

Runs tests

Runs rust tests, codegens csharp sdk and runs csharp tests. This does not include Unreal tests. This expects to run in a clean git state.

**Usage:**
```bash
Usage: test
```

**Options:**

- `--help`: Print help (see a summary with '-h')

### `lint`

Lints the codebase

Runs rustfmt, clippy, csharpier and generates rust docs to ensure there are no warnings.

**Usage:**
```bash
Usage: lint
```

**Options:**

- `--help`: Print help (see a summary with '-h')

### `wasm-bindings`

Tests Wasm bindings

Runs tests for the codegen crate and builds a test module with the wasm bindings.

**Usage:**
```bash
Usage: wasm-bindings
```

**Options:**

- `--help`: Print help (see a summary with '-h')

### `dlls`

Builds and packs C# DLLs and NuGet packages for local Unity workflows

Packs the in-repo C# NuGet packages and restores the C# SDK to populate `sdks/csharp/packages/**`. Then overlays Unity `.meta` skeleton files from `sdks/csharp/unity-meta-skeleton~/**` onto the restored versioned package directory, so Unity can associate stable meta files with the most recently built package.

**Usage:**
```bash
Usage: dlls
```

**Options:**

- `--help`: Print help (see a summary with '-h')

### `smoketests`

Runs smoketests

Executes the smoketests suite with some default exclusions.

**Usage:**
```bash
Usage: smoketests [OPTIONS] [ARGS]... [COMMAND]
```

**Options:**

- `--server`: Run tests against a remote server instead of spawning local servers.

When specified, tests will connect to the given URL instead of starting local server instances. Tests that require local server control (like restart tests) will be skipped.

- `--dotnet`: 
- `args`: 
- `--help`: Print help (see a summary with '-h')

#### `prepare`

Only build binaries without running tests

Use this before running `cargo test --all` to ensure binaries are built.

**Usage:**
```bash
Usage: prepare
```

**Options:**

- `--help`: Print help (see a summary with '-h')

#### `help`

**Usage:**
```bash
Usage: help [COMMAND]...
```

**Options:**

- `subcommand`: 

### `update-flow`

Tests the update flow

Tests the self-update flow by building the spacetimedb-update binary for the specified target, by default the current target, and performing a self-install into a temporary directory.

**Usage:**
```bash
Usage: update-flow [OPTIONS]
```

**Options:**

- `--target`: Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms.
- `--github-token-auth`: Whether to enable github token authentication feature when building the update binary. By default this is disabled.
- `--help`: Print help (see a summary with '-h')

### `cli-docs`

**Usage:**
```bash
Usage: cli-docs [OPTIONS]
```

**Options:**

- `--spacetime-path`: specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)
- `--help`: Print help (see a summary with '-h')

### `self-docs`

**Usage:**
```bash
Usage: self-docs [OPTIONS]
```

**Options:**

- `--check`: Only check for changes, do not generate the docs
- `--help`: Print help (see a summary with '-h')

### `global-json-policy`

**Usage:**
```bash
Usage: global-json-policy
```

**Options:**

- `--help`: 

### `regen`

Regenerate all committed codegen outputs in the repo.

Run this after changing codegen, table schemas, or module definitions to keep committed bindings in sync. Finishes with `cargo fmt --all`.

## What this regenerates

### 1. SDK test bindings (`regen_sdk_test_bindings`)

Rust (5 module/client pairs via `spacetime generate --lang rust`): - modules/sdk-test                    -> sdks/rust/tests/test-client/src/module_bindings/ - modules/sdk-test-connect-disconnect  -> sdks/rust/tests/connect_disconnect_client/src/module_bindings/ - modules/sdk-test-procedure           -> sdks/rust/tests/procedure-client/src/module_bindings/ - modules/sdk-test-view                -> sdks/rust/tests/view-client/src/module_bindings/ - modules/sdk-test-event-table         -> sdks/rust/tests/event-table-client/src/module_bindings/

C# regression tests (3 pairs via `spacetime generate --lang csharp`): - sdks/csharp/examples~/regression-tests/client/module_bindings/ - sdks/csharp/examples~/regression-tests/republishing/client/module_bindings/ - sdks/csharp/examples~/regression-tests/procedure-client/module_bindings/

Unreal (2 pairs via `spacetime generate --lang unrealcpp`): - modules/sdk-test        -> sdks/unreal/tests/TestClient/ - modules/sdk-test-procedure -> sdks/unreal/tests/TestProcClient/

### 2. Demo bindings (`regen_demo_bindings`)

- Blackholio Unity C#: demo/Blackholio/client-unity/Assets/Scripts/autogen/ - Blackholio Unreal C++: demo/Blackholio/client-unreal/

### 3. Template bindings (`regen_template_bindings`)

- C# quickstart-chat: templates/chat-console-cs/module_bindings/ - Rust chat-console-rs: templates/chat-console-rs/src/module_bindings/ - TS templates: auto-discovered via `pnpm -r --filter ./templates/** run generate` - deno-ts: templates/deno-ts/src/module_bindings/ (explicit, not pnpm-discoverable) - Unreal QuickstartChat: sdks/unreal/examples/QuickstartChat/

### 4. SDK internal bindings (`regen_sdk_internal_bindings`)

- TS client-api: crates/bindings-typescript/src/sdk/client_api/ - C# ClientApi: sdks/csharp/src/SpacetimeDB/ClientApi/ - TS test-app: crates/bindings-typescript/test-app/src/module_bindings/ - TS test-react-router-app: crates/bindings-typescript/test-react-router-app/src/module_bindings/

### 5. Moduledef type bindings (`regen_moduledef_bindings`)

- C#:  crates/bindings-csharp/Runtime/Internal/Autogen/ - TS:  crates/bindings-typescript/src/lib/autogen/ - C++: crates/bindings-cpp/include/spacetimedb/internal/autogen/ - Unreal SDK: sdks/unreal/src/SpacetimeDbSdk/Source/SpacetimeDbSdk/Public/ModuleBindings/

## Other codegen not covered by this command

- C++ WASM module (modules/sdk-test-procedure-cpp/client/) — requires Emscripten toolchain - CLI reference docs: `cargo ci cli-docs` - Codegen snapshot tests: `cargo test -p spacetimedb-codegen` (uses insta snapshots, update with `cargo insta review`)

**Usage:**
```bash
Usage: regen [OPTIONS]
```

**Options:**

- `--check`: Only check if bindings are up-to-date, without modifying files.

Regenerates all bindings into a temporary state and then runs `tools/check-diff.sh` to verify nothing changed. Exits with an error if any bindings are stale. Also runs `check_autogen_coverage` to verify all committed autogen directories are covered by the regen script.

- `--help`: Print help (see a summary with '-h')

### `help`

**Usage:**
```bash
Usage: help [COMMAND]...
```

**Options:**

- `subcommand`: 


---

This document is auto-generated by running:

```bash
cargo ci self-docs
```