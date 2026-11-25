# SpacetimeDB

Use SpacetimeDB for application data, accounts, and backend functions. Put the
TypeScript module in the required directory below. Choose the schema, libraries, architecture, and the
rest of the project structure.

## Connection

Use the connection settings below.

| Setting | Value |
|---|---|
| Server URI | `<STDB_URI>` |
| Module name | `<MODULE_NAME>` |
| SpacetimeDB CLI | `<STDB_BIN>` |
| TypeScript SDK package | `<STDB_PACKAGE>` |
| Module source directory | `/app/backend/spacetimedb` |
| Web application | `http://localhost:<APP_PORT>` |

The browser and this environment both reach the server at `<STDB_URI>`.

Publish only the named module to the exact server URI. Local publish and
development commands must use `--yes`. Do not pipe confirmation input, publish
anonymously, or use the hosted service. Create `/app/start.sh`. From a clean
source checkout, it must install dependencies, build the complete application,
and start it on `<APP_PORT>`. The script must not change source files. Leave
the application running when the work is complete.

CLI `--help` is available for command syntax.
