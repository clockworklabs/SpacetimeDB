# spacetimedb-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server for
SpacetimeDB. It exposes SpacetimeDB to MCP-aware agents and editors as a set of
tools, built on the official Rust MCP SDK ([`rmcp`](https://crates.io/crates/rmcp)).

> Status: early scaffold. Only a `ping` health-check tool exists today.
> SpacetimeDB-specific tools (read-only schema and reducer introspection first)
> are being added incrementally.

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

## Run

Point an MCP client at the built binary. Example client config entry:

```json
{
  "mcpServers": {
    "spacetimedb": {
      "command": "/path/to/target/debug/spacetimedb-mcp"
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

| Tool   | Description                                    |
| ------ | ---------------------------------------------- |
| `ping` | Health check. Echoes an optional message back. |

More tools (schema, table, and reducer introspection) will be listed here as
they land.
