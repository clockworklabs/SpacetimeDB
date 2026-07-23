# SpacetimeDB Codex Plugin

A Codex plugin for building on SpacetimeDB: the core data model, the `spacetime` CLI workflow, and
per-language skills for modules and clients in Rust, C#, TypeScript, C++, Unity, and Unreal. It
also registers an MCP server, so the agent can list your databases, read schemas, run SQL, and
call reducers with your CLI login.

Use this when an app needs a real-time backend: tables and reducers, subscriptions, typed client
bindings, multiplayer state, or a running database the agent should inspect and operate.

## Install

```bash
codex plugin marketplace add clockworklabs/SpacetimeDB --sparse .agents --sparse codex-plugin
codex plugin add spacetimedb@spacetimedb-plugins
```

The sparse flags fetch just the catalog and the plugin instead of the whole repository.

From a local checkout, run them from the repository root with `.` as the source. Confirm with
`codex plugin list`. Update with `codex plugin marketplace upgrade`, then re-run the add command.

## Example asks

```text
Set up a new SpacetimeDB module in Rust.
Make the player table public and explain what that changes.
Wire a React client to my database with typed bindings.
List my SpacetimeDB databases and show the schema for chat.
Run SELECT * FROM message on my database.
My client sees no rows. Is the table private?
```

## The skills

Eleven skills, each loaded on demand when its `description` matches the task:

| Skill | Loads when |
| --- | --- |
| `concepts` | learning SpacetimeDB or making an architectural decision |
| `cli` | running the `spacetime` CLI |
| `mcp` | inspecting or operating a live database through the MCP tools |
| `rust-server` | writing tables and reducers in Rust |
| `csharp-server` | writing tables and reducers in C# |
| `typescript-server` | writing tables and reducers in TypeScript |
| `cpp-server` | writing tables and reducers in C++ |
| `typescript-client` | building a TypeScript or React client |
| `csharp-client` | building a C# or .NET client |
| `unity` | building a Unity client |
| `unreal` | building an Unreal Engine client |

Two rules trip up agents most, and the `concepts` skill states them directly: tables are private
by default (clients cannot even see them), and clients change data only by calling reducers, never
by writing tables directly. The per-language skills cover the exact syntax, the common mistakes,
and the CLI workflow.

## Live database tools (MCP)

The plugin registers one MCP server, and it needs no configuration:

```json
{ "spacetimedb": { "command": "spacetime", "args": ["mcp"] } }
```

Two pieces of the repository back this. The host serves MCP over HTTP at `/v1/mcp`, reusing the
same auth and internals as the rest of the API. The `spacetime mcp` subcommand is a stdio to HTTP
bridge to that route, because agents launch MCP servers over stdio.

`spacetime mcp` with no arguments serves the host-wide `/v1/mcp` endpoint of your default server,
and the agent names a database per call: `list_databases` shows the ones you own, then
`get_schema`, `sql`, and `call` each take a `database` argument. Everything runs with your
`spacetime login` identity.

To pin a single database instead, pass it (`"args": ["mcp", "mydb"]`) or set `SPACETIMEDB_DB_NAME`.
The tools then drop the `database` argument.

To use the same server from Claude Desktop or Claude Code, add the command to your own config,
wrapped in a top-level `mcpServers` key, the same shape as this plugin's `.mcp.json`:

```json
{ "mcpServers": { "spacetimedb": { "command": "spacetime", "args": ["mcp"] } } }
```

> Heads up: `spacetime mcp` is UNSTABLE and may not be in your released CLI yet. Build it from this
> repo if needed. If the command is missing, the MCP server simply fails to start and the skills
> keep working on their own.

## Repo contents

- `.agents/plugins/marketplace.json`: the catalog for installing from this directory. A matching
  one at the repository root serves the `clockworklabs/SpacetimeDB` path.
- `plugins/spacetimedb/`: the plugin payload, holding the manifest, `.mcp.json`, `LICENSE`,
  assets, and the skills.
- `plugins/spacetimedb/skills/`: a copy of the repository's `skills/` directory.

## Privacy & data

This plugin collects no data and sends no telemetry. The skills are plain text that the agent
reads, and they run nothing on your machine. The MCP server is the only part that uses the
network, and it connects only to the SpacetimeDB server you choose, using the login you already
have.

## Maintaining

The skills are a copy of the repository's `skills/`, because a plugin must be self-contained (a
symlink installs empty). After changing `skills/`, re-sync:

```bash
# from SpacetimeDB/codex-plugin/
rm -rf plugins/spacetimedb/skills && cp -R ../skills plugins/spacetimedb/skills
```

Then bump `version` in `plugin.json`, since installs are cached by version. Keep the two catalogs
identical apart from `source.path`, which must never point at `./`.

## License

Apache-2.0. See `LICENSE`.
