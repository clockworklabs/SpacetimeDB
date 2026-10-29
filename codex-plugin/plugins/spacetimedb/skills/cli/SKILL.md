---
name: cli
description: SpacetimeDB CLI reference for initializing projects, building modules, publishing databases, querying data, and managing servers
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: shared
  language: all
  cursor_globs: "**/*"
  cursor_always_apply: false
triggers:
  - spacetime init
  - spacetime build
  - spacetime publish
  - spacetime dev
  - spacetime sql
  - spacetime call
  - spacetime logs
  - spacetime server
  - spacetime login
  - spacetime generate
  - spacetime env
  - spacetime auth
  - how do I use the CLI
  - CLI command
---

# SpacetimeDB CLI

Use this skill when the user needs help with the `spacetime` CLI tool - initializing projects, building modules, publishing databases, querying data, managing servers, or troubleshooting CLI issues.

## Quick Reference

### Project Initialization & Development

```bash
# Initialize new project
spacetime init my-project --lang rust|csharp|typescript|cpp
spacetime init my-project --template <template-id>

# Build module
spacetime build                    # release build
spacetime build --debug            # faster iteration, slower runtime

# Dev mode (auto-rebuild, auto-publish, generates bindings)
spacetime dev
spacetime dev --client-lang typescript --module-bindings-path ./client/src/module_bindings

# Generate client bindings
spacetime generate --lang typescript|csharp|rust --out-dir ./bindings --module-path ./server
spacetime generate --lang unrealcpp --uproject-dir ./MyGame --module-path ./server --unreal-module-name MyGame
```

### Client and Module Bindings

`spacetime generate --bindings client` (default) generates bindings for clients connecting to a database. `--bindings module` generates receiver bindings used inside a **calling database module** to call reducers in another **receiving database** through asynchronous IDC. Rust and TypeScript module bindings are supported; C#/C++ module bindings are not implemented yet. `unrealcpp` is client-only; `cpp` is accepted only in module mode.

In `spacetime.json`, each calling database's non-inherited `dependencies` array requires its own `module-path` and dependency `name` plus receiving-module `module-path` or `database`. A `database` reference resolves another target's local source/build settings in the same config; it does not fetch a running database's schema. A direct receiving module source takes precedence. Dependencies do not themselves publish receiving databases or inherit the calling module's build settings. `generate <calling-database>` processes both its regular `generate` entries (client bindings by default) and `dependencies` (module bindings automatically); no `--bindings` flag or second `generate` entry is needed for a dependency. For dependencies, module binding language defaults to the calling module's language and Rust/TypeScript output defaults to `<calling module-path>/src/remote_bindings`. Explicit config `out-dir` is config-relative. A regular `generate` entry can select `"bindings": "module"`, but does not use dependency language or output defaults. The CLI `--bindings` override is allowed only when a single generation entry is selected.

`dev` generates only root dependencies before building the calling module, and watches their source paths. Child targets are available for source lookup, not recursive dependency generation. `--skip-generate` skips receiver/client generation and receiver-only watching; `--skip-publish` does not. See https://spacetimedb.com/docs/cli-reference/spacetime-json#receiver-dependencies for examples.

### Publishing & Deployment

```bash
# Publish to Maincloud (default)
spacetime publish my-database --yes

# Publish to local server
spacetime publish my-database --server local --yes

# Clear database and republish
spacetime publish my-database --delete-data=always --yes
```

### Database Environment

Module declarations validate string values before initialization/migration. Publishing preserves unspecified stored values. For each declared key, precedence is publishing-process environment → resolved config `env` map → stored database value. Undeclared keys can be staged explicitly in JSON, but module code cannot read them until declared. Keep secrets out of checked-in config; use process variables or ignored local config files.

```bash
# Supply declared values while publishing; use a disposable value for local development
API_KEY='development-only' spacetime publish my-database --server local --yes

# Change only the environment of an existing database (no module build/upload)
API_KEY='development-only' spacetime publish my-database --server local --env-only --yes

# Explicitly remove an optional or undeclared key
spacetime publish my-database --server local --env-only --unset-env LOG_LEVEL --yes

# Inspect stored values (these commands print secrets; authorization is required)
spacetime env list my-database --server local
spacetime env get my-database MODE --server local
```

Before `--unset-env`, remove that key from effective JSON/process inputs. Required values cannot be unset. `--replace-env` replaces the whole store and deletes omitted keys; it cannot be combined with `--unset-env`. `--delete-data` also clears environment values; supply required values again after a reset. `--env-only` requires an existing database and cannot be combined with a reset. An empty string is a value, not deletion. `--env` selects a project configuration environment; it does not assign a database environment variable.

There is no separate `spacetime env set`: updates use validated publishing. For full merge/validation semantics see https://spacetimedb.com/docs/databases/environment-variables.

### Database Interaction

```bash
# SQL queries
spacetime sql my-database "SELECT * FROM users"
spacetime sql my-database --interactive   # REPL mode

# Call reducers (each argument is a separate positional arg)
spacetime call my-database my_reducer '"value"' '123'

# Subscribe to changes
spacetime subscribe my-database "SELECT * FROM users" --num-updates 10

# View logs
spacetime logs my-database -f              # follow logs
spacetime logs my-database -n 100          # up to 100 log lines

# Describe schema
# (without --json, output is human-readable text)
spacetime describe my-database --json
spacetime describe my-database tables --json   # also views, reducers, procedures, routes, env, types
spacetime describe my-database tables users --json
spacetime describe my-database reducers my_reducer --json
```

### Database Management

```bash
# List databases
spacetime list

# Delete database
spacetime delete my-database

# Rename database
spacetime rename <database-identity> --to new-name
```

### Server Management

```bash
# List configured servers
spacetime server list

# Add server
spacetime server add local --url http://localhost:3000 --default
spacetime server add myserver --url https://my-spacetime.example.com

# Set default server
spacetime server set-default local

# Test connectivity
spacetime server ping local

# Start local instance
spacetime start

# Clear local data
spacetime server clear
```

### Authentication

```bash
# Login (opens browser)
spacetime login

# Login with token
spacetime login --token <token>

# Show login status
spacetime login show

# Logout
spacetime logout
```

### SpacetimeAuth Management (2.13+)

`spacetime auth` manages SpacetimeAuth settings; it is not a replacement for `spacetime login`. Log in and enable SpacetimeAuth for the database first. Pass the database **name**, not its identity. For example:

```bash
spacetime auth config set my-database display_name 'My App'
spacetime auth idp enable my-database github
spacetime auth client create my-database --name web
spacetime auth client set my-database --name web web true
spacetime auth client get my-database --name web
spacetime auth client set my-database --name web redirect_uris 'https://example.com/callback'
```

The command families are `config set/reset`, `idp set/enable/disable`, and `client create/delete/get/set`. `idp set <database> <idp> <client_id>` prompts for the OAuth secret when omitted (or reads it from stdin); avoid embedding real secrets in shell history. `client get --include-secret` explicitly prints a client's secret; ordinary create/get output does not. These commands use the SpacetimeAuth management service, not the database `--server` target. Use each subcommand's `--help` for supported providers, settings, and values.

## Default Servers

| Name | URL | Description |
|------|-----|-------------|
| `maincloud` | `https://maincloud.spacetimedb.com` | Production cloud (default) |
| `local` | `http://127.0.0.1:3000` | Local development server |

## Common Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--server` | `-s` | Target server (nickname, hostname, or URL) |
| `--yes` | `-y` | Non-interactive mode (skip confirmations) |
| `--anonymous` | | Use anonymous identity |
| `--module-path` | `-p` | Path to module project |
| `--no-config` | | Ignore `spacetime.json` project configuration |

## Troubleshooting

### "Not logged in"
```bash
spacetime login
# Or use --anonymous for public operations
```

### "Server not responding"
```bash
spacetime server ping <server>
# For local: ensure spacetime start is running
```

### "Schema conflict"
```bash
# Clear data and republish
spacetime publish my-db --delete-data=always --yes
```

### "Build failed"
```bash
# Check Rust/C# toolchain
rustup show
# For Rust modules, ensure wasm32-unknown-unknown target
rustup target add wasm32-unknown-unknown
```

## Module Languages

**Server-side (modules):** Rust, C#, TypeScript, C++
**Client SDKs:** TypeScript, C#, Rust, Unreal Engine
**CLI `generate` targets:** TypeScript, C#, Rust, Unreal C++
