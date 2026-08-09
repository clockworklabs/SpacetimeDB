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

**Usage:**
```bash
Usage: test [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `lint`

**Usage:**
```bash
Usage: lint [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `wasm-bindings`

**Usage:**
```bash
Usage: wasm-bindings [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `dlls`

**Usage:**
```bash
Usage: dlls
```

**Options:**

- `--help`: Print help

### `smoketests`

**Help:**
```text
This command builds the binaries needed by the smoketests, then runs them. This prevents race
conditions when running tests in parallel with nextest, where multiple test processes might try to
build the same binaries simultaneously

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

          When specified, tests will connect to the given URL instead of starting local server
          instances. Tests that require local server control (like restart tests) will be skipped.

      --auth-host[=<AUTH_HOST>]
          Use a SpacetimeAuth-issued login for remote-server tests.

          This is required for servers that reject direct server-issued logins for privileged
          operations.

          Optionally accepts an auth host to pass through to `spacetime login`, for example
          `--auth-host=https://spacetimedb.com`.

      --dotnet <DOTNET>
          [default: true]
          [possible values: true, false]

  -h, --help
          Print help (see a summary with '-h')
```

### `keynote-bench`

**Usage:**
```bash
Usage: keynote-bench [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `update-flow`

**Help:**
```text
Usage: ci-update-flow [OPTIONS]

Options:
      --target <TARGET>
          Target triple to build for, by default the current target. Used by github workflows to
          check the update flow on multiple platforms.

      --github-token-auth
          Whether to enable github token authentication feature when building the update binary. By
          default this is disabled.

  -h, --help
          Print help (see a summary with '-h')
```

### `cli-docs`

**Help:**
```text
Usage: ci-cli-docs [OPTIONS]

Options:
      --spacetime-path <SPACETIME_PATH>
          specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is
          located)

  -h, --help
          Print help (see a summary with '-h')
```

### `dep-check`

**Usage:**
```bash
Usage: dep-check [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `self-docs`

**Usage:**
```bash
Usage: self-docs [OPTIONS]
```

**Options:**

- `--check`: Only check for changes, do not generate the docs
- `--help`: Print help

### `global-json-policy`

**Usage:**
```bash
Usage: global-json-policy [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `publish-checks`

**Usage:**
```bash
Usage: publish-checks [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `typescript-test`

**Usage:**
```bash
Usage: typescript-test [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `version-upgrade-check`

**Usage:**
```bash
Usage: version-upgrade-check [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `docs`

**Usage:**
```bash
Usage: docs [ARGS]...
```

**Options:**

- `args <ARGS>`: Arguments forwarded to the split CI command package
- `--help`: Print help

### `other-workflows`

**Usage:**
```bash
Usage: other-workflows <COMMAND>
```

**Options:**

- `--help`: Print help

#### `coordinate-internal-tests`

**Help:**
```text
Selects or starts the private workflow for a public Internal Tests run

Usage: ci-coordinate-internal-tests [OPTIONS] --public-sha <PUBLIC_SHA>

Options:
      --public-sha <PUBLIC_SHA>
          Immutable public commit to test
      --public-pr-number <PUBLIC_PR_NUMBER>
          Public pull request number, when coordinating a pull request run
  -h, --help
          Print help
```

#### `codeowners-check`

**Help:**
```text
Usage: ci-codeowners-check --base-ref <BASE_REF> --pr-number <PR_NUMBER>

Options:
      --base-ref <BASE_REF>    Git ref to compare against, usually origin/<pull request base branch>
      --pr-number <PR_NUMBER>  Pull request number to inspect for approval state
  -h, --help                   Print help
```

#### `cla-assistant`

**Help:**
```text
Usage: ci-cla-assistant <COMMAND>

Commands:
  retry   Retries CLA Assistant if `license/cla` is the only remaining PR blocker
  status  Returns the `license/cla` status for a pull request or commit SHA
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

#### `help`

**Usage:**
```bash
Usage: help [COMMAND]...
```

**Options:**

- `subcommand <COMMAND>`: Print help for the subcommand(s)

### `help`

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