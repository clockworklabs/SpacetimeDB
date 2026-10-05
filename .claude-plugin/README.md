# SpacetimeDB for Claude Code

`marketplace.json` here installs the repository's `skills/` directory and the SpacetimeDB MCP
server into Claude Code:

```bash
claude plugin marketplace add clockworklabs/SpacetimeDB
claude plugin install spacetimedb@spacetimedb-plugins
```

From a local checkout, use `./` as the source. Confirm with `claude plugin details spacetimedb`,
which lists the skills and the MCP server. The MCP server runs `spacetime mcp`, which bridges
stdio to the HTTP endpoint on whichever server your CLI is configured for.

## Maintaining

The entry uses `strict: false`, so it reads `skills/` directly and needs no manifest inside a
plugin payload. It lists each skill explicitly, so adding one under `skills/` means adding it
here too. `cargo ci lint` fails when the list and the directory disagree. Validate the catalog
with `claude plugin validate .`.
