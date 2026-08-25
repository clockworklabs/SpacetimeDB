# SpacetimeDB TypeScript Components

Reusable packages for SpacetimeDB TypeScript modules. Submodules run inside a
module, own or extend transactional state, and use `ctx.http.fetch` when they
need an external API. Browser clients connect directly to SpacetimeDB.

Mountable submodules target the released SpacetimeDB 2.8 TypeScript SDK and
CLI. Package peer dependencies accept compatible 2.x releases from 2.8.3
onward. Repository development and release verification use version 2.8.3.

## Start here

- **Adding a component to an application:** follow
  [Getting started](./COMPONENTS_GETTING_STARTED.md), then use the
  package-specific README.
- **Evaluating the components:** choose a runnable application from the package
  table. Example READMEs include the exact local database, port, credentials,
  and first successful action.
- **Contributing to this repository:** use the repository-development workflow
  below.

A typical mountable component starts with:

```bash
npm install @spacetimedb/rate-limit spacetimedb@^2.8.3
```

```ts
import { schema } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';

const spacetimedb = schema({ rateLimit });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  rateLimit.installRateLimit(ctx.as.rateLimit);
});
```

The host application owns authorization and exposes operations and views for
its users. The package READMEs and full examples show that boundary.

## Packages

| Package                                                 | Purpose                                                         | Runnable example                                      |
| ------------------------------------------------------- | --------------------------------------------------------------- | ----------------------------------------------------- |
| [`@spacetimedb/agents`](./spacetime-agents-ts/)         | Agent definitions, typed tools, model providers, and embeddings | [Multi-provider chat](./spacetime-agents-ts/example/) |
| [`@spacetimedb/api-keys`](./spacetime-api-keys-ts/)     | API key issuance, verification, rotation, and audit history     | [Colony sharing](./spacetime-api-keys-ts/example/)    |
| [`@spacetimedb/auth`](./spacetime-auth-ts/)             | Password and OAuth authentication, sessions, and profiles       | [Authenticated notes](./spacetime-auth-ts/example/)   |
| [`@spacetimedb/cron`](./spacetime-cron-ts/)             | Durable calendar and interval scheduling                        | [Cron dashboard](./spacetime-cron-ts/example/)        |
| [`@spacetimedb/crypto`](./spacetime-crypto-ts/)         | Hashing, encoding, and webhook-signature helpers                | Used by the provider examples                         |
| [`@spacetimedb/files`](./spacetime-files-ts/)           | Transactional file storage, visibility, and serving             | [Vault](./spacetime-files-ts/example/)                |
| [`@spacetimedb/grid`](./spacetime-grid-ts/)             | Square and hex grids, pathfinding, ranges, and movement         | [Grid Tactics](./spacetime-grid-ts/example/)          |
| [`@spacetimedb/lobby`](./spacetime-lobby-ts/)           | Queues, rooms, ranked matching, and match results               | [Starclash](./spacetime-lobby-ts/example/)            |
| [`@spacetimedb/posthog`](./spacetime-posthog-ts/)       | PostHog capture, outbox delivery, and feature flags             | [Context Cafe](./spacetime-posthog-ts/example/)       |
| [`@spacetimedb/presence`](./spacetime-presence-ts/)     | Presence, heartbeat, activity, and expiration                   | [Presence Chat](./spacetime-presence-ts/example/)     |
| [`@spacetimedb/rate-limit`](./spacetime-rate-limit-ts/) | Fixed-window rate limiting and bounded sweeps                   | [Powerhouse](./spacetime-rate-limit-ts/example/)      |
| [`@spacetimedb/resend`](./spacetime-resend-ts/)         | Resend email delivery and signed webhook ingestion              | [Dispatch](./spacetime-resend-ts/example/)            |
| [`@spacetimedb/retry`](./spacetime-retry-ts/)           | Typed retry dispatch, backoff, and attempt history              | [Cron dashboard](./spacetime-cron-ts/example/)        |
| [`@spacetimedb/stripe`](./spacetime-stripe-ts/)         | Stripe catalog, checkout, billing state, and webhooks           | [Premium Store](./spacetime-stripe-ts/example/)       |

Each package ships TypeScript source, a BUSL-1.1 license, API documentation,
and a runnable integration example where the submodule needs host-module
wiring.

## Submodule model

Mountable packages expose `./submodule`. That entrypoint exports the schema,
registered operations, views, and an `install<Name>` helper. The host module
owns lifecycle hooks and route registration.

Host-configured packages such as `agents`, `cron`, and `retry` keep
application-specific dispatch typed in the consuming module. Root and
documented subpath exports provide pure helpers.

Shared rules:

- Secrets live in private tables, never in public procedure arguments.
- Admin identities are seeded from the publishing owner during initialization.
- Per-user and per-membership data stays private and is exposed through scoped
  views.
- Reducers use context time and randomness so execution remains deterministic.
- Scheduled work is bounded per invocation and leaves observable history.

See [Component authoring](./COMPONENTS_AUTHORING.md) for package conventions.

## Repository development

Install the official SpacetimeDB launcher, select 2.8.3, and install workspace
dependencies before running the repository gates:

```bash
spacetime version install 2.8.3
spacetime version use 2.8.3
pnpm install
```

The build gate rejects any CLI or embedded library version other than 2.8.3.
Component manifests use pnpm workspace references to the repository SDK.

Package checks:

```bash
pnpm components:check
pnpm components:build
pnpm components:consumer:check
```

`components:consumer:check` packs all 14 releases, installs them into a clean
temporary project with `spacetimedb@2.8.3`, resolves every public export, and
builds a host module containing all mountable components.

Start the released standalone server with `spacetime start` when running an
integration example. Examples use the standard `local` server alias and port
`3000`.

To publish and test every example with disposable local databases:

```bash
pnpm components:browser:install
pnpm components:smoke:examples:ephemeral
```

This command requires an active local server and CLI login. It creates a unique
database for each example, generates client bindings, builds the browser app,
checks the HTTP surface, opens the app in Chromium, and exercises one safe UI
interaction. It fails on browser errors, unexpected HTTP errors, and missing
static assets. It removes each disposable database after the test. Provider
credentials are not used.

Use the named-database command only when you intend to replace the normal local
example databases:

```bash
pnpm components:smoke:examples:fresh
```

This command publishes with `--delete-data=always`. Its script name and required
confirmation flag make the data deletion explicit. Use each provider package's
opt-in test separately when validating real credentials.

Run only the server and HTTP checks when Chromium is unavailable:

```bash
pnpm components:smoke:examples:http:fresh
```

With a local SpacetimeDB server running, the Stripe and Resend synthetic smoke
tests need no provider credentials. Credentialed provider tests, such as
Stripe's sandbox E2E suite, are documented in the corresponding package
README.
