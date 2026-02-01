---
name: spacetimedb-cli
description: SpacetimeDB CLI reference for initializing projects, building modules, publishing databases, querying data, and managing servers
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
  - how do I use the CLI
  - CLI command
---

# SpacetimeDB CLI

Use this skill when the user needs help with the `spacetime` CLI tool - initializing projects, building modules, publishing databases, querying data, managing servers, or troubleshooting CLI issues.

## Quick Reference

### Project Initialization & Development

```bash
# Initialize new project
spacetime init my-project --lang rust|csharp|typescript
spacetime init my-project --template <template-id>

# Build module
spacetime build                    # release build
spacetime build --debug            # faster iteration, slower runtime

# Dev mode (auto-rebuild, auto-publish, generates bindings)
spacetime dev
spacetime dev --client-lang typescript --module-bindings-path ./client/src/module_bindings

# Generate client bindings
spacetime generate --lang typescript|csharp|rust|unrealcpp --out-dir ./bindings
```

### Publishing & Deployment

```bash
# Publish to Maincloud (default)
spacetime publish my-database --yes

# Publish to local server
spacetime publish my-database --server local --yes

# Publish with data handling
spacetime publish my-database --delete-data always      # always clear data
spacetime publish my-database --delete-data on-conflict # clear only if schema conflicts
spacetime publish my-database --delete-data never       # never clear (default)

# Allow breaking client changes
spacetime publish my-database --break-clients
```

### Database Interaction

```bash
# SQL queries
spacetime sql my-database "SELECT * FROM users"
spacetime sql my-database --interactive   # REPL mode

# Call reducers
spacetime call my-database my_reducer '{"arg1": "value", "arg2": 123}'

# Subscribe to changes
spacetime subscribe my-database "SELECT * FROM users" --num-updates 10

# View logs
spacetime logs my-database -f              # follow logs
spacetime logs my-database -n 100          # last 100 lines

# Describe schema
spacetime describe my-database --json
spacetime describe my-database table users --json
spacetime describe my-database reducer my_reducer --json
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
spacetime server add local http://localhost:3000 --default
spacetime server add myserver https://my-spacetime.example.com

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

### Energy/Billing

```bash
spacetime energy balance
spacetime energy balance --identity <identity>
```

## Default Servers

| Name | URL | Description |
|------|-----|-------------|
| `maincloud` | `https://spacetimedb.com` | Production cloud (default) |
| `local` | `http://127.0.0.1:3000` | Local development server |

## Common Workflows

### New Project Setup

```bash
# 1. Login
spacetime login

# 2. Create project
spacetime init my-game --lang rust
cd my-game

# 3. Start dev mode (auto-rebuilds and publishes)
spacetime dev
```

### Local Development

```bash
# Start local server (in separate terminal)
spacetime start

# Publish to local
spacetime publish my-db --server local --delete-data always --yes

# Query local database
spacetime sql my-db --server local "SELECT * FROM players"
```

### Generate Client Bindings

```bash
# After building module
spacetime build
spacetime generate --lang typescript --out-dir ./client/src/bindings

# Or use dev mode which auto-generates
spacetime dev --client-lang typescript --module-bindings-path ./client/src/bindings
```

## Common Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--server` | `-s` | Target server (nickname, hostname, or URL) |
| `--yes` | `-y` | Non-interactive mode (skip confirmations) |
| `--anonymous` | | Use anonymous identity |
| `--identity` | `-i` | Specify identity to use |
| `--project-path` | `-p` | Path to module project |

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
spacetime publish my-db --delete-data always --yes
```

### "Build failed"
```bash
# Check Rust/C# toolchain
rustup show
# For Rust modules, ensure wasm32-unknown-unknown target
rustup target add wasm32-unknown-unknown
```

## Module Languages

**Server-side (modules):** Rust, C#, TypeScript
**Client SDKs:** TypeScript, C#, Rust, Python, Unreal Engine

## Notes

- Many commands are marked UNSTABLE and may change
- Default server is `maincloud` unless configured otherwise
- Use `--yes` flag in scripts to avoid interactive prompts
- Dev mode watches files and auto-rebuilds on changes
