# Backend: SpacetimeDB

Use **SpacetimeDB** as the backend. Make the remaining implementation choices
yourself.

## Available backend access

| Setting | Value |
|---|---|
| Server URI | `<STDB_URI>` |
| Module name | `<MODULE_NAME>` |
| SpacetimeDB CLI | `<STDB_BIN>` |
| TypeScript SDK package | `<STDB_PACKAGE>` |
| Client dev server | must be reachable at `http://localhost:<VITE_PORT>` |

Publish only the named module to the exact server URI. Local publish and
development commands must use `--yes`; do not pipe confirmation input, publish
anonymously, or use the hosted service. Leave the client running on the stated
port when finished. The included TypeScript server and client SDK references
describe the available APIs. CLI `--help` is available for command syntax.

## Branding & Styling

- App title: **"SpacetimeDB <APP_NOUN>"**
- Dark theme using official SpacetimeDB brand colors:
  - Primary: `#4cf490`
  - Primary hover: `#4cf490bf`
  - Secondary: `#a880ff`
  - Background: `#0d0d0e`
  - Surface: `#141416`
  - Border: `#202126`
  - Text: `#e6e9f0`
  - Text muted: `#6f7987`
  - Accent: `#02befa`
  - Success: `#4cf490`
  - Warning: `#fbdc8e`
  - Danger: `#ff4c4c`
