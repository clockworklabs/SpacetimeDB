# Presence chat example

A chat app with rooms, messages, and online status. Use two accounts to see
when someone is online, typing, or reading a conversation.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-presence-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://localhost:8794>.

## Try it

1. Create an account, then create a server. Here, a server is a group of chat rooms.
2. Open a private browser window and create a second account.
3. Use **Add a server** to join the first account's server, then open its
   `general` room.
4. Send a message and start typing a reply. Watch the typing indicator and
   online status in the other window.
5. Try a reaction, a reply, and a small attachment.
6. Change one account's status to invisible. The other account sees it as offline.

You can also create a private room. A room administrator must add people before
they can read its messages or attachments.

After someone closes the app, their online status may take about 30 seconds
to expire.

## Configuration

See [.env.example](./.env.example) for the server and optional Google/GitHub
sign-in settings. Keep `AUTH_ISSUER_URL` and `AUTH_BASE_URL` set to the address you open
in the browser. Restart the example server after changes.

## Troubleshooting

- **Startup configuration fails:** use the CLI account that published the database.
- **Sign-in fails after a database reset:** clear this site's browser data and
  create an account again.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): chat and presence actions.
- [spacetimedb/src/chat-policy.ts](./spacetimedb/src/chat-policy.ts): presence and activity limits.
- [public/ui.js](./public/ui.js): rooms and message controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-presence-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
