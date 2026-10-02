# Colony API-key example

Build a colony and share a link that lets someone view or edit it.
Choose what each link permits, then replace or revoke it.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-api-keys-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8798>.

## Try it

1. Choose **Viewer** and click **Create share link**.
2. Copy the link while it is shown. Open it in a private browser window.
3. The second browser can view the colony but cannot edit it.
4. Create a **Builder** link and open that link instead. It permits placing
   and removing structures.
5. Click **New link** for a key. The old link stops working.
6. Click **Revoke**. The person using that link loses access through it.

A link is shown only when you create or replace it. If you lose it, use
**New link** to issue a replacement.

## Permissions

- **Viewer:** read the colony.
- **Terraformer:** change terrain.
- **Builder:** place and remove structures.
- **Planter:** place natural objects.
- **Collaborator:** all of those actions.

## Configuration

See [.env.example](./.env.example) for the server settings. Leave
`API_KEYS_SECRET` blank for local use: the example server creates it for a new
database and keeps the stored value on restart.

## Before deploying

Anyone with a share link can use its permissions until it expires or is revoked.
Do not log keys or full share URLs.

The colony's world data is publicly readable by colony ID in this demo.
The links control edits; they do not make the world data confidential.
Add read-access checks if your app needs private resources.

## Troubleshooting

- **Link opens the wrong page:** copy the complete URL, including `#key=...`.
- **Edits are rejected:** check the link's role, expiry, and whether the owner
  replaced or revoked it.
- **Startup is not authorized:** use the CLI account that published the database.
- **Browser and HTTP requests show different data:** check that `STDB_URI`,
  `STDB_HTTP`, and `STDB_SERVER` address the same database server.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): colony actions and key checks.
- [spacetimedb/src/roles.ts](./spacetimedb/src/roles.ts): permissions for each role.
- [src/app.ts](./src/app.ts): share links and map controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-api-keys-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
