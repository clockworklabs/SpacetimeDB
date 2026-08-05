# Backend: SpacetimeDB

Use **SpacetimeDB** as the backend. Your server logic is a module published to
it; the client connects and subscribes. How you structure the module and the
client is your choice.

## What the harness needs

| Setting | Value |
|---|---|
| Server URI | `http://localhost:3000` |
| Module name | `<MODULE_NAME>` |
| Client dev server | must be reachable at `http://localhost:<VITE_PORT>` |

Publish the module and regenerate the client bindings from it:

```bash
spacetime publish <MODULE_NAME> --module-path <your-module-dir>
spacetime generate --lang typescript --out-dir <your-client>/src/module_bindings --module-path <your-module-dir>
```

Republish after any server change. If a schema change is rejected as
incompatible, republish with `--delete-data`. `spacetime logs <MODULE_NAME>`
shows module output including reducer errors.

The client must be served on that port and left running when you are done.

## Branding & Styling

- App title: **"SpacetimeDB Chat"**
- Dark theme using official SpacetimeDB brand colors:
  - Primary: `#4cf490` (SpacetimeDB green)
  - Primary hover: `#4cf490bf` (green 75% opacity)
  - Secondary: `#a880ff` (SpacetimeDB purple)
  - Background: `#0d0d0e` (shade2 — near black)
  - Surface: `#141416` (shade1 — slightly lighter)
  - Border: `#202126` (n6)
  - Text: `#e6e9f0` (n1 — light gray)
  - Text muted: `#6f7987` (n4)
  - Accent: `#02befa` (SpacetimeDB blue)
  - Success: `#4cf490` (green — same as primary)
  - Warning: `#fbdc8e` (SpacetimeDB yellow)
  - Danger: `#ff4c4c` (SpacetimeDB red)
  - Gradient (optional, for headers): `linear-gradient(266deg, #4cf490 0%, #8a38f5 100%)` (green to purple)
