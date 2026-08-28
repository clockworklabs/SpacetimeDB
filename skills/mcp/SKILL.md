---
name: mcp
description: Operate a running SpacetimeDB database through MCP tools rather than the CLI - list databases, read schemas, run SQL, and call reducers. Use when the client exposes spacetimedb MCP tools and the task is to inspect or change data in a live database.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: shared
  language: all
  cursor_globs: "**/*"
  cursor_always_apply: false
triggers:
  - inspect the database
  - query the database
  - list my databases
  - read the schema
  - call a reducer
  - use the MCP server
  - spacetimedb MCP
---

# SpacetimeDB over MCP

A SpacetimeDB host speaks MCP, so an MCP-aware client can operate a live database with tool calls
instead of shell commands. If the client exposes `spacetimedb` tools, use them for anything that
reads or changes a running database.

Check the tool list before deciding they are missing. The tools are passive: they appear as
`spacetimedb.list_databases`, `spacetimedb.get_schema`, `spacetimedb.sql`, `spacetimedb.call`, and
`spacetimedb.ping`, and nothing announces them.

## Which to reach for

| Task | Use |
| --- | --- |
| List databases, read a schema, run SQL, call a reducer | **MCP tools** |
| init, build, publish, generate, start, logs | **`spacetime` CLI** (see the `cli` skill) |

The MCP tools only operate a database that already exists. They cannot scaffold a project, compile a
module, publish, or generate bindings, so the two are complementary rather than alternatives. Prefer
the tools when they are present: they are typed, they return JSON, and the client can gate the
destructive ones. When no MCP client is attached, the equivalent CLI commands are correct.

## The tools

| Tool | Arguments | Returns |
| --- | --- | --- |
| `list_databases` | none | the databases **you own**, with identity and names |
| `get_schema` | `database` | tables and reducers as JSON |
| `sql` | `database`, `sql`, optional `confirmed` | rows as JSON |
| `call` | `database`, `reducer`, optional `args` (JSON array) | the reducer outcome |
| `ping` | optional `message` | a health check |

`list_databases` lists only your own databases, so it is empty for an anonymous identity. Start there
when you do not know the database name.

## Two shapes, so read `tools/list` first

A server is either host-wide or scoped to one database. Do not assume which:

**Host-wide** (`spacetime mcp` with no database, or `POST /v1/mcp`). Every data tool takes a required
`database` argument, a name or an identity, and `list_databases` is offered:

```json
{ "name": "sql", "arguments": { "database": "mydb", "sql": "SELECT * FROM message" } }
```

**Scoped** (`spacetime mcp <database>`, or `POST /v1/database/<db>/mcp`). The connection fixes the
database, so there is no `database` argument and no `list_databases`:

```json
{ "name": "sql", "arguments": { "sql": "SELECT * FROM message" } }
```

## Rules that do not change

The tools run with your identity, exactly as the HTTP API does. The model is the same as everywhere
else, so the `concepts` skill still governs:

1. **Reducers are the write path.** Use `call` to change data. It runs in a transaction that either
   fully commits or fully rolls back.
2. **SQL writes require ownership.** `sql` reads public tables; writing through it needs you to own
   the database. Prefer `call`.
3. **Private tables are not client-readable.** `get_schema` still shows a private table's
   declaration, so a `no such table` error from `sql` usually means the table is private rather than
   missing. Access depends on your identity, so do not assume a private table is readable.
4. **Tool errors come back in band.** A failed reducer or a bad query returns a result with
   `isError: true` and the message as text, not a transport failure. Read the text before retrying.

## Inspecting a database

```
list_databases {}
get_schema     { "database": "mydb" }
sql            { "database": "mydb", "sql": "SELECT * FROM message" }
call           { "database": "mydb", "reducer": "send_message", "args": ["hello"] }
```

Pass `"confirmed": true` to `sql` to wait for a durably confirmed read.

## Troubleshooting

| Message | Meaning |
| --- | --- |
| `database argument must be a string` | The server is host-wide and you omitted `database` |
| `unknown tool: list_databases` | The server is scoped to one database already |
| `` `x` not found `` | No such database on this server, or a name where an identity was meant |
| `no such table: x` | The table is private, or you are querying the wrong database |
| No `spacetimedb` tools at all | No MCP server is connected. Use the CLI instead (`cli` skill) |

`spacetime mcp` is UNSTABLE and may not be in a released CLI yet, so a client that cannot start it
falls back to the CLI commands in the `cli` skill.

Reference: https://spacetimedb.com/docs
