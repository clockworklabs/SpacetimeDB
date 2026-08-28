# SpacetimeDB

Use SpacetimeDB for the application data. Choose the schema, client
architecture, application behavior, and project structure.

## Connection

| Setting | Value |
|---|---|
| Server URI | `<STDB_URI>` |
| Module name | `<MODULE_NAME>` |
| SpacetimeDB CLI | `<STDB_BIN>` |
| TypeScript SDK package | `<STDB_PACKAGE>` |
| Module source directory | `/app/backend/spacetimedb` |
| Web application | `http://localhost:<VITE_PORT>` |

Put the TypeScript module in `/app/backend/spacetimedb`. Publish only the named
module to the exact server URI. Local publish and development commands must use
`--yes`. Do not pipe confirmation input, publish anonymously, or use the hosted
service. Leave the web application running on the stated port when the work is
complete.

The included TypeScript server and client SDK references describe the available
APIs. CLI `--help` is available for command syntax.
