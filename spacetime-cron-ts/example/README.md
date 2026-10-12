# Cron example

A dashboard for scheduling recurring tasks and viewing their results.

The example starts with two tasks:

- **Digest** adds a summary to the activity log at 9 AM on weekdays, New York time.
- **Cleanup** runs every five minutes and trims the activity log to its newest
  50 entries. You can change how many entries it keeps.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-cron-ts/example`, run:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8788>. The two tasks appear on the first publish.

## Try it

1. Click **Schedule job** and select **Digest**.
2. Choose **Every 10 seconds**, then click **Schedule job** to save.
3. Wait for a run to appear. Each run adds an entry to the activity log.
4. Select **Cleanup**, choose **Every 10 seconds**, and set **Rows to keep** to
   `3`. After enough digest runs, cleanup removes the older activity entries.
5. Click **Unschedule** on a task to stop it. Schedule it again to restart it.

You can set a fixed interval, such as every 30 seconds, or a calendar schedule,
such as weekdays at 9 AM. Saving a schedule replaces the previous one for that
task. Schedules keep running when you close the browser, as long as SpacetimeDB
is running.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts) defines the two tasks,
  their starting schedules, and what each task does.
- [src/app.ts](./src/app.ts) connects the dashboard to SpacetimeDB and handles
  the controls.

After changing the server code, run `pnpm run build:module`. This keeps existing
data and schedules. Restart `pnpm run dev` after changing the browser code.

To restore the starting schedules, run `pnpm run build:module:fresh`. **This
deletes all data in the local `spacetime-cron-example` database.**

For instructions on adding scheduled tasks to your own app, see the
[Cron quick start](../README.md#quick-start).

## Configuration

See [.env.example](./.env.example) to change the database address or web-server
port. The defaults work without a `.env` file.

## Before deploying

Anyone who connects to this example can change schedules and read the activity
and run history. Add permission checks and restrict access to that data before
using these controls in a public app.

The example also checks for interrupted schedules every five minutes. For the
details of failed tasks and recovery, see
[Cron's execution model](../README.md#execution-model).

## Troubleshooting

- **Nothing runs:** the starting digest schedule only runs on weekday mornings.
  Use **Every 10 seconds** to see a result sooner.
- **A task stops running:** check its recent failed runs before scheduling it
  again. Repeated failures can disable a task.
