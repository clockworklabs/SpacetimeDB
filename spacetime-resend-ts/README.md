# @spacetimedb/resend

A SpacetimeDB submodule for transactional email via [Resend](https://resend.com):
admin-gated outbound delivery, idempotent webhook ingest, private delivery
state, synchronous procedures, and valibot-validated webhook payloads.

---

## Install

```bash
npm install @spacetimedb/resend spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This submodule can be published directly as its own SpacetimeDB module from the root entry point.

## Usage

### Integrate into an application

Register Resend in the host schema, initialize its private state, and expose only
application-authorized send procedures and caller-scoped delivery views:

```ts
import { schema } from 'spacetimedb/server';
import * as resend from '@spacetimedb/resend/submodule';

const spacetimedb = schema({ resend });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  resend.installResend(ctx.as.resend);
});
```

Provider configuration must run as the publishing owner or a registered Resend
administrator. See the
[Dispatch host module](./example/spacetimedb/)
for a narrow send procedure, scoped views, and signed webhook routing.

### Standalone configuration

Resend credentials live in a private `resend_config` singleton. During `init`, a
fresh database seeds the owner into the private `resend_admin_identity` table.

```bash
spacetime call --server http://127.0.0.1:3000 resend-ts set_resend_config \
  '"re_..."' \
  '{"some":"whsec_..."}' \
  '{"some":"onboarding@resend.dev"}'
```

Args: `apiKey`, `webhookSigningSecret` (required for webhook ingest), `defaultFrom` (optional).
For `t.option(...)` CLI arguments, use `null` for no value and
`{"some":"value"}` for a string value.

Verify:

```bash
spacetime call --server http://127.0.0.1:3000 resend-ts get_resend_config_status '{}'
```

`send_email` reads the Resend API key from private module state.

## Private tables

| Table                   | Key         | Notes                                                        |
| ----------------------- | ----------- | ------------------------------------------------------------ |
| `resend_email`          | `resend_id` | one row per outbound email; `status` reflects latest webhook |
| `resend_delivery_event` | `event_id`  | append-only audit log of every event for an email            |

- `resend_webhook_event` - idempotency log
- `resend_config` - credentials singleton
- `resend_admin_identity` - admin allowlist

None of the Resend base tables are subscribable. Email recipients, subject,
body, tracking state, webhook payloads, and signature headers remain private.
Host modules should expose caller- or tenant-scoped views over `userId` or
`orgId`; the included example demonstrates this pattern.

## API

**Setup**

- `set_resend_config(apiKey, webhookSigningSecret, defaultFrom)`
- `get_resend_config_status()` - `{ isConfigured, hasWebhookSigningSecret, apiKeyLength, ... }`
- `add_admin_identity(identity)` / `remove_admin_identity(identity)`

**Outbound**

- `send_email({ from, to, subject, html, text, cc, bcc, replyTo, tagsJson, headersJson, scheduledAt, idempotencyKey})` - admin-gated; inserts a `resend_email` row with `status = queued`, returns `{ resendId }`. The first webhook flips it to `sent`/`delivered`.
- `cancel_email({ resendId })` - admin-gated
- `resend_api_request({ method, path, jsonBody, idempotencyKey })` - admin-gated
  request to a relative `api.resend.com` path; accepted methods are `GET`, `POST`,
  `PATCH`, and `DELETE`

Email sends accept up to 100 combined `to`, `cc`, and `bcc` recipients. Address
fields are capped at 320 characters, subjects at 998 characters, HTML and text
at 200,000 characters each, and tag or header JSON at 16 KiB. Control characters
in address, subject, and schedule fields are rejected before provider HTTP.

Host modules should prefer the submodule helper export:

```ts
import * as resend from '@spacetimedb/resend/submodule';

resend.sendEmail(ctx.as.resend, {
  to: ['delivered@resend.dev'],
  subject: 'Welcome',
  html: '<p>Hello.</p>',
  tagsJson: JSON.stringify({ userId: 'u_123', orgId: 'launch' }),
});
```

That lets the host app own product-specific authorization and workflow while
the submodule owns config, delivery rows, and webhook ingest.

Expose that helper through a product-facing procedure with recipient policy and
rate limits. The generated client then calls the wrapper:

```ts
const result = await conn.procedures.sendDispatch({
  to: 'delivered@resend.dev',
  subject: 'Welcome',
  message: 'Your workspace is ready.',
});

if (!result.ok) throw new Error(result.message);
```

Subscribe to a caller-scoped host view for delivery status. Keep the private
Resend tables and generic administrative send operation restricted to
operators.

**Webhook ingest / replay**

- `ingest_resend_webhook(eventId, eventType, payloadJson, signatureHeader, timestampHeader)` - idempotent
- `replay_webhook_event(eventId)` - re-applies a stored event
- `makeResendWebhookHandler()` builds a direct HTTP webhook handler for a host
  router.

Webhook application is atomic. Invalid event data rolls back the event row and
email-state changes so Resend can redeliver the event.

**Admin queries**

- `get_email`, `list_emails_by_user_id`, `list_emails_by_org_id`, `list_emails_by_status`
- `list_delivery_events_for_email`

List procedures return at most 1,000 rows. Host applications should expose
caller-scoped, paginated views for product-facing history.

Package entrypoints:

- `@spacetimedb/resend` can run as a standalone email database.
- `@spacetimedb/resend/submodule` supplies submodule configuration, delivery,
  webhook, and query helpers.

## Webhook events handled

```
email.sent              email.delivered
email.delivery_delayed  email.bounced
email.complained        email.failed
email.opened            email.clicked
```

`opened` / `clicked` / `complained` are recorded as **flags + timestamps** and leave `status` unchanged. Terminal states (`delivered`, `bounced`, `failed`, `cancelled`) and in-flight states (`queued`, `sent`, `delivery_delayed`) live in the `status` column.

## Webhook signature verification

The module's `ingest_resend_webhook` reducer verifies Standard Webhooks signatures (svix) using the configured `webhookSigningSecret`. `signatureHeader` and `timestampHeader` are stored on `resend_webhook_event` for forensic replay.

## Tagging

`userId` / `orgId` are extracted from Resend `tags` and indexed for per-user / per-org listing. Both tag shapes are accepted:

- Object form: `{"userId": "u_123", "orgId": "o_456"}`
- Array form (Resend's webhook output): `[{"name":"userId","value":"u_123"}]`

Pass `tagsJson` as a JSON string when calling `send_email`.

## Integration testing

```bash
# Build + publish + event paths, idempotency, replay, signed-type checks, and authorization checks
pnpm run test:smoke
```

The smoke test publishes only to the dedicated `resend-ts-smoke-test` database.

For real Resend test-mode:

```bash
# Bootstrap once with your real key:
spacetime call --server http://127.0.0.1:3000 resend-ts set_resend_config '"re_..."' null '{"some":"onboarding@resend.dev"}'

# Then send to one of Resend's test addresses (delivered@/bounced@/complained@):
spacetime call --server http://127.0.0.1:3000 resend-ts send_email \
  null \
  '["delivered@resend.dev"]' \
  '"Test from SpacetimeDB"' \
  '{"some":"<p>Hello.</p>"}' \
  null null null null \
  '{"some":"{\"userId\":\"u_123\"}"}' \
  null null null
```

To exercise inbound webhooks end-to-end, expose your local relay via ngrok, register the URL in Resend's dashboard with the same `whsec_...` you passed to `set_resend_config`, and forward verified events to `ingest_resend_webhook`.

## Architecture notes

- **valibot for runtime validation.** `vEmailEvent` is a `v.variant('type', [...])` over the 8 supported event types. The unit and smoke suites lock down the accepted wire shapes.
- **Synchronous HTTP.** Procedures are synchronous and `ctx.http.fetch` returns a `SyncResponse`, so `callResend` in `src/submodule/http.ts` implements the required API surface directly.
- **Wire format.** The public input uses SDK-style camelCase (`replyTo`, `scheduledAt`), and `buildSendEmailBody` emits the provider's snake_case JSON fields.
- **Idempotency.** Each webhook event is keyed by `event.id`; re-ingest is a no-op. Status-changing events ratchet forward, so `email.complained` preserves a terminal `delivered` status.

## Testing

```bash
pnpm test
pnpm run lint
```

Credentialed smoke coverage is described in **Integration testing** above.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
