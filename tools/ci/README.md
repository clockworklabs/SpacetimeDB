# SpacetimeDB's cargo ci

## Overview

This document provides an overview of the `cargo ci` command-line tool, and documentation for each of its subcommands and options.

## `cargo ci`

**Usage:**
```bash
cargo ci [--skip <COMMAND>] [COMMAND] [ARGS]...
```

### `test`

**Usage:**
```bash
cargo ci test
```

### `lint`

**Usage:**
```bash
cargo ci lint
```

### `wasm-bindings`

**Usage:**
```bash
cargo ci wasm-bindings
```

### `dlls`

**Usage:**
```bash
cargo ci dlls
```

### `smoketests`

**Usage:**
```bash
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
cargo ci keynote-bench
```

### `update-flow`

**Usage:**
```bash
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

**Usage:**
```bash
Usage: ci-cli-docs [OPTIONS]

Options:
      --spacetime-path <SPACETIME_PATH>
          specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is
          located)

  -h, --help
          Print help (see a summary with '-h')
```

### `self-docs`

**Usage:**
```bash
cargo ci self-docs [--check]
```

### `global-json-policy`

**Usage:**
```bash
cargo ci global-json-policy
```

### `publish-checks`

**Usage:**
```bash
cargo ci publish-checks
```

### `typescript-test`

**Usage:**
```bash
cargo ci typescript-test
```

### `version-upgrade-check`

**Usage:**
```bash
cargo ci version-upgrade-check
```

### `docs`

**Usage:**
```bash
cargo ci docs
```

### `other-workflows`

**Usage:**
```bash
cargo ci other-workflows <COMMAND>
```

#### `coordinate-internal-tests`

**Usage:**
```bash
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

**Usage:**
```bash
Usage: ci-codeowners-check --base-ref <BASE_REF> --pr-number <PR_NUMBER>

Options:
      --base-ref <BASE_REF>    Git ref to compare against, usually origin/<pull request base branch>
      --pr-number <PR_NUMBER>  Pull request number to inspect for approval state
  -h, --help                   Print help
```

#### `cla-assistant`

**Usage:**
```bash
Usage: ci-cla-assistant <COMMAND>

Commands:
  retry   Retries CLA Assistant if `license/cla` is the only remaining PR blocker
  status  Returns the `license/cla` status for a pull request or commit SHA
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

##### `retry`

**Usage:**
```bash
Retries CLA Assistant if `license/cla` is the only remaining PR blocker

Usage: ci-cla-assistant retry [OPTIONS] --pr-number <PR_NUMBER>

Options:
      --pr-number <PR_NUMBER>  Pull request number to check
      --repo <REPO>            Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY
  -h, --help                   Print help
```

##### `status`

**Usage:**
```bash
Returns the `license/cla` status for a pull request or commit SHA

Usage: ci-cla-assistant status [OPTIONS] <--pr <PR>|--sha <SHA>>

Options:
      --pr <PR>      Pull request number whose head commit should be checked
      --sha <SHA>    Commit SHA to check
      --repo <REPO>  Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY
  -h, --help         Print help
```

---

This document is auto-generated by running:

```bash
cargo ci self-docs
```
