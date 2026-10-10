# Dispatch email example

Send an email with Resend and watch its delivery status update in the app.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-resend-ts/example`, copy [.env.example](./.env.example) to `.env`.

You also need a Resend API key. In `.env`, replace `RESEND_API_KEY` with your
key. Leave `RESEND_WEBHOOK_SECRET` empty until you set up a webhook below.
Remove the sample `RESEND_ALLOWED_RECIPIENTS` address or replace it with an
address you control.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8790>.

## Try it

1. Click **Delivered** to use Resend's test recipient.
2. Write a subject and message, then send the email.
3. The message appears in the list. To see it change to Delivered, complete
   the webhook setup below.
4. Try the **Bounced** and **Complaint** test recipients to see other outcomes.

To send to a real address, add it to
`RESEND_ALLOWED_RECIPIENTS` and restart the example server. Use a sender that
Resend permits for your account; `DEFAULT_FROM` starts as
`onboarding@resend.dev`.

## Receive delivery updates

1. Use a tunnel to expose the example's `/webhook/resend` route.
2. Add that public URL as a webhook endpoint in Resend and select the email
   events you want to receive.
3. Copy the endpoint's signing secret into `RESEND_WEBHOOK_SECRET`.
4. Restart `pnpm run dev` and send another test email.

The local route is `http://127.0.0.1:8790/webhook/resend`. You can also forward
directly to the database route:

```text
http://127.0.0.1:3000/v1/database/spacetime-resend-example/route/webhook/resend
```

The app rejects webhook requests with a missing or invalid signature.
Without a reachable webhook, email status stays Queued.

## Before deploying

Anyone who can open this demo can send to the allowed recipients. It limits
each browser identity to five sends per ten minutes and all users to 25 per
hour. Add account-based access checks before making the app public. Expose
only the webhook route when testing through a tunnel.

The server saves its admin credential in `.stdb-server-token`. Keep that file
and `.env` private.

## Troubleshooting

- **Sending fails:** check the API key, allowed recipients, and sender address.
- **Status stays Queued:** check the public webhook URL and signing secret.
- **`resend.not_authorized`:** use the CLI account that published the database,
  then restart the example server.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): email sending and delivery updates.
- [server.ts](./server.ts): Resend setup and webhook forwarding.
- [src/app.ts](./src/app.ts): email form and message list.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-resend-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
