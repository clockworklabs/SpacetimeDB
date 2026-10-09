# Grid tactics example

A turn-based game on a hex map. Move units, attack enemies, and play against
the computer or another person.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-grid-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://localhost:8793>.

## Try it

1. Create an account and click **Deploy solo (vs aliens)**.
2. Select one of your units. Choose a reachable cell to move it.
3. Attack an enemy in range, then click **End turn**.
4. Wait for the computer to take its turn. Continue until the match ends.
5. For a two-player match, create a human match. Open a private browser window,
   create a second account, and join it.

Closing or reloading the page does not stop the computer's turn.

Leaving an active match counts as a forfeit. A waiting match is removed if
nobody joins it within 30 minutes.

## Configuration

See [.env.example](./.env.example) for the server and optional Google/GitHub
sign-in settings. Keep `AUTH_ISSUER_URL` and `AUTH_BASE_URL` set to the address you open
in the browser. Restart the example server after changes.

## Troubleshooting

- **Startup configuration fails:** use the CLI account that published the database.
- **A unit cannot move:** check whose turn it is, movement range, and blocked
  or occupied cells.
- **Sign-in fails after a database reset:** clear this site's browser data and
  create an account again.
- **OAuth redirects incorrectly:** make the provider callback match the browser
  address and port in your auth settings.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): match rules and the computer opponent.
- [spacetimedb/src/views.ts](./spacetimedb/src/views.ts): which matches each player can see.
- [public/ui.js](./public/ui.js): map display and game controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-grid-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
