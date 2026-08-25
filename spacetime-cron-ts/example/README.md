# Cron example

This example is a small browser dashboard backed directly by SpacetimeDB. It shows two jobs:

- `digest`, a weekday calendar job in `America/New_York`
- `cleanup`, a five-minute native interval job with a typed `keep` argument

The dashboard subscribes to the sanitized `cron_jobs` view, exact calendar targets, interval estimates, run outcomes, and application activity. The private job rows retain typed arguments without exposing them to browser subscriptions. Controls can reschedule or disable either job. Selecting `cleanup` also supplies its typed row-retention argument when scheduling.

This is a local development example. Its scheduling reducers accept any connected caller so the browser can exercise the component. Add application authorization before deploying equivalent controls. The example also enables `publicTables`, which exposes run history and trigger state for the dashboard. Review that visibility before using the same option in an application.

## What this demonstrates

- Declaring static calendar and interval jobs.
- Attaching a typed argument to the `cleanup` job.
- Registering one handler and one schedule table for each job.
- Scheduling defaults during a fresh database initialization.
- Rescheduling and disabling jobs through application reducers.
- Subscribing to sanitized job state, run history, and application activity.
- Repairing an enabled job that loses its pending fire.
- Keeping browser-visible job state separate from private typed arguments.

## Prerequisites

- Node.js 20 or later
- pnpm 10
- SpacetimeDB CLI 2.8.3 or later
- A local SpacetimeDB server

Select the supported CLI release:

```powershell
spacetime version install 2.8.3
spacetime version use 2.8.3
```

## Quick start

Start the server in a separate terminal:

```bash
spacetime start
```

From this directory:

```bash
pnpm install
pnpm --dir spacetimedb install
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8788>.

`build:module:fresh` performs four steps:

1. Builds the TypeScript module.
2. Publishes `spacetime-cron-example` to the local server with fresh data.
3. Regenerates TypeScript client bindings.
4. Typechecks and bundles the browser application.

After a module edit, preserve existing local data with:

```bash
pnpm run build:module
```

## Use in your project

This workspace tests the component source in this repository. Consumer
applications install the published release:

```bash
npm install @spacetimedb/cron spacetimedb@^2.8.3
```

The complete server integration is in [`spacetimedb/src/index.ts`](./spacetimedb/src/index.ts). The browser integration is in [`src/app.ts`](./src/app.ts).

## Configuration

The static server reads these optional variables from the process environment or `.env` files:

| Variable            | Default                  | Purpose                              |
| ------------------- | ------------------------ | ------------------------------------ |
| `PORT`              | `8788`                   | Development web-server port.         |
| `HOST`              | `127.0.0.1`              | Development web-server bind address. |
| `STDB_URI`          | `ws://127.0.0.1:3000`    | Browser WebSocket endpoint.          |
| `STDB_APP_DATABASE` | `spacetime-cron-example` | Published database name.             |

The browser connects directly to SpacetimeDB. The Express process serves static files and `/api/config`.

## Execution flow

1. `cronTable()` declares `digest` and `cleanup` as static jobs. `cleanup` also
   declares `{ keep: u32 }` as its durable argument.
2. `createCron()` creates the private shared job table, run history, one schedule
   table for each job, and the optional five-minute reconciler.
3. `schema()` mounts the Cron tables with the example's `activity_log` table.
4. `cronReducer()` binds each job to its static handler.
5. `cron.reconcileReducer()` registers the low-frequency lost-fire repair sweep.
6. `init` schedules the weekday digest and five-minute cleanup defaults on a
   fresh publish.
7. The browser subscribes to `cron_jobs`, `cron_run`, and `activity_log`.
8. The scheduling form calls application reducers. Those reducers validate the
   selected job and typed cleanup argument before calling `schedule()`.

The `digest` handler appends a summary row. The `cleanup` handler keeps the newest
configured number of activity rows and records how many older rows it removed.

## Failure and recovery behavior

Reducer jobs run in a transaction. A successful job commits its application
writes, run record, health update, and next calendar fire together.

When a reducer handler throws, the fire transaction rolls back. Cron uses the
temporary SpacetimeDB volatile primitive to call the same job reducer with an
internal recovery payload, then rethrows the original error. The second
invocation restores the calendar schedule and records the failed run in a new
transaction. It does not run the application handler again. Native interval
rows remain present, and the same recovery path records their failure state.

The volatile request is best effort. A host failure can leave an enabled job
without a pending fire. This example enables a five-minute reconciler. The
reconciler repairs that invariant and records a failed run with `lost_fire`.
Calls to `schedule()` and `unschedule()` also perform this repair
opportunistically.

Typed arguments remain in the private `cron_job` row. The browser-visible
`cron_jobs` view omits them. This example exposes fire tables and detailed run
history through `publicTables` for demonstration. Applications should expose
only the status required by their users.

## Authorization and deployment boundaries

- `scheduleCron`, `scheduleEvery`, and `unscheduleJob` are open so a local browser
  can exercise the component. Production modules must authorize these calls.
- Job names and argument types are part of the database schema. Changing an
  existing argument type requires a schema migration or a new job name.
- Reducer handlers must remain deterministic. Use a Cron procedure for HTTP or
  other procedure capabilities.
- Procedure handlers should use `CronInvocation.id` as an external idempotency
  key.
- The included Express process is a local static server. Production needs TLS,
  explicit origin policy, and process supervision.

## Build and verification

From `spacetime-cron-ts/example`:

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

From `spacetime-cron-ts`, run the package and local integration suites:

```powershell
pnpm test
pnpm run test:module:local
pnpm run test:recovery
```

`test:module:local` requires a running local SpacetimeDB server and publishes a
temporary database. `test:recovery` starts and removes its own isolated server.
Together, they exercise reducer rollback, same-reducer volatile recovery,
calendar and interval failures, typed arguments, lost-fire reconciliation,
history limits, and host restart recovery.

For a browser release check:

1. Confirm both seeded jobs appear after a fresh publish.
2. Schedule `digest` as a short interval and confirm a run and activity row
   appear.
3. Schedule `cleanup` with a new `keep` value and confirm the active schedule
   remains after reload.
4. Unschedule a job and confirm it becomes disabled with no pending fire.
5. Reschedule the disabled job and confirm it fires again.
6. Confirm the browser console has no errors and every subscription applies.

## Troubleshooting

- **No jobs appear:** confirm `STDB_APP_DATABASE` matches
  `spacetime-cron-example` and reload after the subscription applies.
- **The browser cannot connect:** confirm `STDB_URI` points to the server used by
  `spacetime publish --server local`.
- **A local package edit is missing:** run
  `pnpm --dir spacetimedb install --force`, then rebuild the module.
- **A schedule is rejected:** use a five-field or six-field Cron expression, a
  valid IANA time zone, or an interval from 1 through 31,536,000 seconds.
- **A job disables itself:** inspect its recent failed runs and configured
  failure threshold before rescheduling it.

## Important files

- `spacetimedb/src/index.ts` - job declarations, handlers, initialization, and
  browser-facing management reducers.
- `src/app.ts` - typed connection, subscriptions, rendering, and controls.
- `public/index.html` - dashboard structure.
- `public/styles.css` - dashboard presentation.
- `server.ts` - static development server and browser-safe configuration.
