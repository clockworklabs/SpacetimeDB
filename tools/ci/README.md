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

- `--skip <SKIP>`: Skip specified subcommands when running all

When no subcommand is specified, all subcommands are run in sequence. This option allows specifying subcommands to skip when running all. For example, to skip the `unreal-tests` subcommand, use `--skip unreal-tests`.

- `--help`: Print help (see a summary with '-h')

### `test`

Runs tests

Runs rust tests, codegens csharp sdk and runs csharp tests. This does not include Unreal tests. This expects to run in a clean git state.

Usage: ci-test

Options:
  -h, --help
          Print help (see a summary with '-h')

### `lint`

Lints the codebase

Runs rustfmt, clippy, csharpier, TypeScript lint, and generates rust docs to ensure there are no warnings.

Usage: ci-lint

Options:
  -h, --help
          Print help (see a summary with '-h')

### `wasm-bindings`

Tests Wasm bindings

Runs tests for the codegen crate and builds a test module with the wasm bindings.

Usage: ci-wasm-bindings

Options:
  -h, --help
          Print help (see a summary with '-h')

### `dlls`

Deprecated; use `cargo regen csharp dlls`

**Usage:**
```bash
Usage: dlls
```

**Options:**

- `--help`: Print help

### `smoketests`

Runs smoketests

Executes the smoketests suite with some default exclusions.

Usage: ci-smoketests [OPTIONS] [ARGS]... [COMMAND]

Commands:
  prepare         Only build binaries without running tests
  check-mod-list
  help            Print this message or the help of the given subcommand(s)

Arguments:
  [ARGS]...
          Additional arguments to pass to the test runner

Options:
      --server <SERVER>
          Run tests against a remote server instead of spawning local servers.

          When specified, tests will connect to the given URL instead of starting local server instances. Tests that require local server control (like restart tests) will be skipped.

      --auth-host[=<AUTH_HOST>]
          Use a SpacetimeAuth-issued login for remote-server tests.

          This is required for servers that reject direct server-issued logins for privileged operations.

          Optionally accepts an auth host to pass through to `spacetime login`, for example `--auth-host=https://spacetimedb.com`.

      --dotnet <DOTNET>
          [default: true]
          [possible values: true, false]

  -h, --help
          Print help (see a summary with '-h')

### `keynote-bench`

Runs the keynote benchmark as a CI performance regression gate.

Assumes release SpacetimeDB binaries and the TypeScript SDK are already built, runs the keynote SpacetimeDB benchmark for 60 seconds against the TypeScript and Rust modules, and fails if throughput is below 275K TPS for TypeScript or 300K TPS for Rust.

Usage: ci-keynote-bench

Options:
  -h, --help
          Print help (see a summary with '-h')

### `update-flow`

Tests the update flow

Tests the self-update flow by building the spacetimedb-update binary for the specified target, by default the current target, and performing a self-install into a temporary directory.

Usage: ci-update-flow [OPTIONS]

Options:
      --target <TARGET>
          Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms.

      --github-token-auth
          Whether to enable github token authentication feature when building the update binary. By default this is disabled.

  -h, --help
          Print help (see a summary with '-h')

### `cli-docs`

Generates CLI documentation and checks for changes

Usage: ci-cli-docs [OPTIONS]

Options:
      --spacetime-path <SPACETIME_PATH>
          specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)

  -h, --help
          Print help (see a summary with '-h')

### `self-docs`

**Usage:**
```bash
Usage: self-docs [OPTIONS]
```

**Options:**

- `--check`: Only check for changes, do not generate the docs
- `--help`: Print help

### `global-json-policy`

Verify that any non-root global.json files are symlinks to the root global.json

Usage: ci-global-json-policy

Options:
  -h, --help  Print help

### `publish-checks`

Checks that publishable crates satisfy publish constraints

Usage: ci-publish-checks

Options:
  -h, --help  Print help

### `typescript-test`

Runs TypeScript workspace tests and template build checks

Usage: ci-typescript-test

Options:
  -h, --help  Print help

### `version-upgrade-check`

Verifies that the repository version upgrade tool still works

Usage: ci-version-upgrade-check

Options:
  -h, --help  Print help

### `docs`

Builds the docs site

Usage: ci-docs-build

Options:
  -h, --help  Print help

### `other-workflows`

**Usage:**
```bash
Usage: other-workflows <COMMAND>
```

**Options:**

- `--help`: Print help

#### `coordinate-internal-tests`

Selects or starts the private workflow for a public Internal Tests run.

Usage: ci-coordinate-internal-tests [OPTIONS] --public-sha <PUBLIC_SHA>

Options:
      --public-sha <PUBLIC_SHA>              Immutable public commit to test
      --public-pr-number <PUBLIC_PR_NUMBER>  Public pull request number, when coordinating a pull request run
  -h, --help                                 Print help

#### `codeowners-check`

Checks that sensitive CODEOWNERS-controlled files have the required approvals.

Usage: ci-codeowners-check --base-ref <BASE_REF> --pr-number <PR_NUMBER>

Options:
      --base-ref <BASE_REF>    Git ref to compare against, usually origin/<pull request base branch>
      --pr-number <PR_NUMBER>  Pull request number to inspect for approval state
  -h, --help                   Print help

#### `cla-assistant`

Interacts with CLA Assistant.

Usage: ci-cla-assistant <COMMAND>

Commands:
  retry   Retries CLA Assistant if `license/cla` is the only remaining PR blocker
  status  Returns the `license/cla` status for a pull request or commit SHA
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

#### `help`

Print this message or the help of the given subcommand(s)

**Usage:**
```bash
Usage: help [COMMAND]...
```

**Options:**

- `subcommand <COMMAND>`: Print help for the subcommand(s)

### `help`

Print this message or the help of the given subcommand(s)

**Usage:**
```bash
Usage: help [COMMAND]...
```

**Options:**

- `subcommand <COMMAND>`: Print help for the subcommand(s)


---

This document is auto-generated by running:

```bash
cargo ci self-docs
```