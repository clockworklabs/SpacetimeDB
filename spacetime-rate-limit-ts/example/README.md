# Powerhouse rate-limit example

A reactor game that shows how to limit repeated actions. Tap the reactor to
generate energy, buy upgrades, and watch what happens when you act too quickly.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-rate-limit-ts/example`, run:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8792>.

## Try it

1. Tap the reactor to generate energy. Watch **Tap Charges** decrease.
2. Keep tapping until you reach the limit. Wait for the charges to refill, then
   tap again.
3. Watch the heat meter too. Overheating can stop your taps even when you have
   charges left. Heat is a separate game rule.
4. Spend energy on upgrades and try the other actions. They have their own
   limits and cooldowns.
5. Open the app in a private browser window to join as another player. Players
   have separate tap limits, while the reactor and upgrade purchases are shared.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts) sets the limits and
  checks them when a player acts.
- [spacetimedb/src/reactor-rules.ts](./spacetimedb/src/reactor-rules.ts) defines
  the game rules and upgrades.
- [src/app.ts](./src/app.ts) connects the browser to SpacetimeDB.
- [public/ui.js](./public/ui.js) handles the game display and controls.

After changing the server code, run `pnpm run build:module`. This keeps current
players and upgrades. Restart `pnpm run dev` after changing the browser code.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in the
local `spacetime-rate-limit-example` database.**

For instructions on limiting actions in your own app, see the
[Rate Limit integration guide](../README.md#integrate-into-an-application).

## Configuration

See [.env.example](./.env.example) to change the database address or web-server
port. The defaults work without a `.env` file.

## Optional admin controls

The CLI identity that first publishes the database is its initial administrator.
To grant access to your browser identity, run this command while logged in as
that administrator. Replace `BROWSER_IDENTITY_HEX` with the connected browser's
identity:

```bash
spacetime call --server local spacetime-rate-limit-example rate_limit.add_rate_limit_admin 0x<BROWSER_IDENTITY_HEX>
```

## Before deploying

This game uses browser identities to distinguish players. In an app with user
accounts, connect the limits to those accounts so a user cannot get a fresh
allowance by opening a new browser session. Keep permission checks on the server;
rate limits do not decide who is allowed to perform an action.

## Troubleshooting

- **Taps are rejected:** check both **Tap Charges** and reactor heat. Either can
  stop an action.
- **Debug data is empty:** grant your browser identity admin access using the
  command above.
