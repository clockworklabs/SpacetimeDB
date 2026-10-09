# Auth notes example

Create an account and keep private notes. Try password sign-in, session
management, and optional Google or GitHub sign-in.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-auth-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://localhost:8791>.

## Try it

1. Create an account and add a note.
2. Edit the note, then reload the page. You should still be signed in and see
   the updated text.
3. Open a private browser window and create a second account. Its notes start empty.
4. Open **Active sessions** to see where you are signed in and revoke a session.

For password reset and email verification, this demo writes links to the
SpacetimeDB module logs instead of sending email. To read them, run:

```bash
spacetime logs --server local spacetime-auth-example
```

Treat these links as secrets. Anyone with a valid link can use it.

## Optional Google or GitHub sign-in

Register an app with the provider and put its client ID and secret in `.env`.
For the default address, use these callback URLs:

- Google: `http://localhost:8791/auth/google/callback`
- GitHub: `http://localhost:8791/auth/github/callback`

Restart the example server after changing these settings.

## Configuration

See [.env.example](./.env.example) for all settings. Keep `AUTH_ISSUER_URL`
and `AUTH_BASE_URL` set to the address you open in the browser.
Use `localhost` consistently; `127.0.0.1` has separate browser cookies.

Leave `AUTH_ES256_PRIVATE_KEY_PEM` blank for local use. The example server
creates a signing key for a new database and keeps the existing key on restart.

## Before deploying

Replace the log-based mailer with real email delivery. Stop logging one-time
links, and store the signing key in durable secret storage.

## Troubleshooting

- **Startup configuration fails:** use the CLI account that published the database.
- **Sign-in is lost on reload:** check that the page address matches the auth
  settings, including hostname and port.
- **OAuth redirect mismatch:** check the registered callback URL against the
  URLs above, including the port.
- **Sessions fail after a database reset:** clear this site's browser data and
  create an account again.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): sign-in routes, notes, and the log-based mailer.
- [src/app.ts](./src/app.ts): sign-in and live note updates.
- [public/ui.js](./public/ui.js): notes and account controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-auth-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
