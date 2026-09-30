# @spacetimedb/resend

A SpacetimeDB submodule for transactional email via [Resend](https://resend.com):
admin-gated outbound delivery, idempotent webhook ingest, private delivery
state, synchronous procedures, and valibot-validated webhook payloads.

---

## Install

```bash
npm install @spacetimedb/resend spacetimedb
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
  resend.install(ctx.as.resend);
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
spacetime call --server http://127.0.0.1:3000 spacetime-resend set_resend_config \
  '"re_..."' \
  '{"some":"whsec_..."}' \
  '{"some":"onboarding@resend.dev"}'
```

Args: `apiKey`, `webhookSigningSecret` (required for webhook ingest), `defaultFrom` (optional).
For `t.option(...)` CLI arguments, use `null` for no value and
`{"some":"value"}` for a string value.

Verify:

```bash
spacetime call --server http://127.0.0.1:3000 spacetime-resend get_resend_config_status '{}'
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
- `get_resend_config_status()` - `{ isConfigured, hasWebhookSecret, apiKeyLength, ... }`
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

resend.sendEmailRequest(ctx.as.resend, {
  to: ['delivered@resend.dev'],
  subject: 'Welcome',
  html: '<p>Hello.</p>',
  tagsJson: JSON.stringify([
    { name: 'userId', value: 'u_123' },
    { name: 'orgId', value: 'launch' },
  ]),
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

The HTTP handler returns 200 after applying an event, for an already processed
event, and for signed event types the submodule does not handle, which are stored
as `Ignored`. Invalid payloads return 400 and retain a failed event record;
redelivery retries failed records. Unexpected transaction failures propagate to
the HTTP runtime.

The reducer throws, writing nothing, when metadata, size, headers, secret, or
signature checks reject a request. Once a signed event is stored, the reducer
commits: an invalid payload keeps its `Failed` event record with the error
message. Callers read the outcome with `get_webhook_event`.

Delivery timestamps use the provider's event time. Older events cannot replace
newer status, and an earlier delivery stage cannot replace a later one. Queued
send responses preserve any webhook state already recorded. Opens, clicks, and
complaints update their own fields without changing delivery status. Error,
bounce, and failure details change only when their event sets the status.

**Admin queries**

- `get_email`, `list_emails_by_user_id`, `list_emails_by_org_id`, `list_emails_by_status`
- `list_delivery_events_for_email`
- `get_webhook_event` - stored event with `status` (`Received`, `Processed`,
  `Ignored`, or `Failed`) and `errorMessage`

List procedures return at most 1,000 rows. Host applications should expose
caller-scoped, paginated views for product-facing history.

**Errors**

`errors` from `@spacetimedb/resend/submodule` lists the `resend.*` codes the
submodule throws.

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

Other signed event types, such as `email.received`, `contact.*`, and `domain.*`,
are acknowledged and stored as `Ignored`.

`opened` / `clicked` / `complained` are recorded as **flags + timestamps** and leave `status` unchanged. Terminal states (`delivered`, `bounced`, `failed`, `cancelled`) and in-flight states (`queued`, `sent`, `delivery_delayed`) live in the `status` column.

## Webhook signature verification

The `ingest_resend_webhook` reducer and the `makeResendWebhookHandler()` route verify Standard Webhooks signatures (svix) using the configured `webhookSigningSecret`. A rejected signature returns 401 with `resend.webhook_signature_mismatch:<reason>`, where the reason is a `@spacetimedb/crypto` `errors` code such as `crypto.timestamp_outside_tolerance`. `signatureHeader` and `timestampHeader` are stored on `resend_webhook_event` for forensic replay.

## Tagging

`userId` / `orgId` are extracted from Resend `tags` and indexed for per-user / per-org listing.

- `send_email` takes `tagsJson` as a JSON string in the array form Resend's send API expects: `[{"name":"userId","value":"u_123"}]`.
- Resend webhook payloads carry tags in object form: `{"userId": "u_123", "orgId": "o_456"}`.

## Integration testing

```bash
# Build + publish + event paths, idempotency, replay, signed-type checks, and authorization checks
pnpm run test:smoke
```

The smoke test publishes only to the dedicated `resend-ts-smoke-test` database.

For real Resend test-mode:

```bash
# Bootstrap once with your real key:
spacetime call --server http://127.0.0.1:3000 spacetime-resend set_resend_config '"re_..."' null '{"some":"onboarding@resend.dev"}'

# Then send to one of Resend's test addresses (delivered@/bounced@/complained@):
spacetime call --server http://127.0.0.1:3000 spacetime-resend send_email \
  null \
  '["delivered@resend.dev"]' \
  '"Test from SpacetimeDB"' \
  '{"some":"<p>Hello.</p>"}' \
  null null null null \
  '{"some":"[{\"name\":\"userId\",\"value\":\"u_123\"}]"}' \
  null null null
```

The standalone module has no HTTP route. To receive webhooks from Resend, mount the submodule in a host module that registers `makeResendWebhookHandler()` on a router, as the [example](./example/spacetimedb/) does at `/webhook/resend`. Expose the database through a public tunnel such as ngrok and register `https://<tunnel>/v1/database/<database>/route/webhook/resend` in Resend's dashboard with the same `whsec_...` you passed to `set_resend_config`.

## Architecture notes

- **valibot for runtime validation.** `vEmailEvent` is a `v.variant('type', [...])` over the 8 supported event types; other event types are stored as `Ignored`. The unit and smoke suites lock down the accepted wire shapes.
- **Synchronous HTTP.** Procedures are synchronous and `ctx.http.fetch` returns a `SyncResponse`, so `callResend` in `src/submodule/http.ts` implements the required API surface directly.
- **Wire format.** The public input uses SDK-style camelCase (`replyTo`, `scheduledAt`), and `buildSendEmailBody` emits the provider's snake_case JSON fields.
- **Idempotency.** Each webhook event is keyed by its `svix-id` header (the reducer's `eventId` argument); re-ingesting a processed or ignored event is a no-op. Status-changing events ratchet forward, so `email.complained` preserves a terminal `delivered` status.

## Testing

```bash
pnpm test
pnpm run lint
```

Credentialed smoke coverage is described in **Integration testing** above.

## License

Apache-2.0.
