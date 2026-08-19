# spacetimedb-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server for
SpacetimeDB. It exposes SpacetimeDB to MCP-aware agents and editors as a set of
tools, built on the official Rust MCP SDK ([`rmcp`](https://crates.io/crates/rmcp)).

> Status: early. Read-only schema introspection works today (`get_schema`,
> `list_tables`, `list_reducers`); a `ping` health check is also included.
> Further tools (SQL, subscriptions, the CLI workflow) are deferred for now.

## Transport

The server speaks JSON-RPC over **stdio**: an MCP client launches it as a
subprocess and exchanges messages on stdin/stdout. Logs go to **stderr only** —
stdout is reserved for the protocol stream, so anything else printed there would
corrupt it.

## Build

```bash
cargo build -p spacetimedb-mcp
```

The binary lands at `target/debug/spacetimedb-mcp`.

## Test

```bash
cargo test -p spacetimedb-mcp
```

Unit tests cover the schema-to-output transformations and the
serialize/deserialize round trip the client relies on; an integration test
serves a canned schema over a throwaway HTTP server and checks the full
fetch-and-decode path, so no running SpacetimeDB instance is required.

## Run

Point an MCP client at the built binary. The introspection tools talk to a
running SpacetimeDB host, configured via environment variables:

| Variable            | Default                 | Purpose                                            |
| ------------------- | ----------------------- | -------------------------------------------------- |
| `SPACETIMEDB_HOST`  | `http://127.0.0.1:3000` | Base URL of the SpacetimeDB host to query.         |
| `SPACETIMEDB_TOKEN` | _(unset)_               | Bearer token, required only for private databases. |

The target database (a name or identity) is passed as an argument to each tool,
so one server can introspect any database on the host. Example client config:

```json
{
  "mcpServers": {
    "spacetimedb": {
      "command": "/path/to/target/debug/spacetimedb-mcp",
      "env": { "SPACETIMEDB_HOST": "http://127.0.0.1:3000" }
    }
  }
}
```

## Smoke test

Drive the JSON-RPC handshake by hand to confirm the round trip works:

```bash
printf '%s\n' \
'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
'{"jsonrpc":"2.0","method":"notifications/initialized"}' \
'{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ping","arguments":{"message":"hi"}}}' \
| ./target/debug/spacetimedb-mcp 2>/dev/null
```

The `tools/call` response should echo `pong: hi`.

## Tools

| Tool            | Arguments  | Description                                                          |
| --------------- | ---------- | -------------------------------------------------------------------- |
| `ping`          | `message?` | Health check. Echoes an optional message back.                       |
| `get_schema`    | `database` | Full module definition (typespace, tables, reducers) as JSON.        |
| `list_tables`   | `database` | Names of all tables in the database.                                 |
| `list_reducers` | `database` | Reducers in the database, with each reducer's lifecycle role if any. |

All introspection is read-only. Write operations, SQL, and subscriptions are
intentionally out of scope for now.
