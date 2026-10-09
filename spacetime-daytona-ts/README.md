# @spacetimedb/daytona

Run commands in temporary Daytona sandboxes from your SpacetimeDB application.
Daytona provides the Linux environment. This submodule manages sandbox requests,
tracks command results, and limits how many sandboxes each owner can use.

The [example app](./example/README.md) lets you create a sandbox, run commands,
and watch the results in your browser. Creating a sandbox can use paid Daytona
credits.

## Setup

This package is not yet published. Use it within this repository with the
workspace SDK and matching SpacetimeDB CLI and server built from this checkout.

Add `@spacetimedb/daytona` and `spacetimedb` to your host module's dependencies
with version `workspace:*`. Build the SDK from the repository root:

```bash
pnpm install
pnpm --filter spacetimedb build
```

You need a Daytona API key with sandbox management and execution access. Set
`DAYTONA_API_KEY` in the shell used to publish the host module. Keep the key on
the server and within the same Daytona organization when rotating it.

## Add it to a module

Mount the submodule and add a timer to check requests every three seconds:

```ts
import {
  schema,
  t,
  ScheduleAt,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as daytona from '@spacetimedb/daytona/submodule';
import { client, daytonaTick } from '@spacetimedb/daytona';

const spacetimedb = schema(
  { daytona, daytonaTick },
  {
    env: { DAYTONA_API_KEY: t.string() },
  }
);
export default spacetimedb;

const sandboxes = client({
  submodule: (tx: TransactionCtx<InferSchema<typeof spacetimedb>>) =>
    tx.as.daytona,
  namespace: 'daytona',
  snapshot: 'daytona-small',
  ttlMinutes: 10,
  maxSandboxesPerOwner: 2,
});

export const init = spacetimedb.init(ctx => {
  ctx.db.daytonaTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(3_000_000n),
  });
});

export const reconcileDaytona = spacetimedb.procedure(
  { onSchedule: daytonaTick },
  { tick: daytonaTick.rowType },
  t.unit(),
  ctx => {
    sandboxes.reconcile(ctx, ctx.env.DAYTONA_API_KEY);
    return {};
  }
);
```

## Application operations

Call these methods inside authorized host reducers. Resolve `owner` from the
authenticated caller. Do not accept an owner value supplied by the browser.

| Method                     | Arguments                                     | Result                     |
| -------------------------- | --------------------------------------------- | -------------------------- |
| `createSandbox(tx, input)` | `owner`, `requestKey`                         | Local sandbox ID           |
| `runCommand(tx, input)`    | `owner`, `sandboxId`, `requestKey`, `command` | Local execution ID         |
| `deleteSandbox(tx, input)` | `owner`, `sandboxId`                          | Records a deletion request |

These methods record requests in the reducer's transaction. The scheduled
procedure sends them to Daytona after commit. See the
[example module](./example/spacetimedb/src/index.ts) for reducers with access
checks and views that return each caller's records.

Generate a request key once per user action. Reuse it if the client repeats that
request. Reusing a command request key with different command text is rejected.
`command` accepts shell text. Quote untrusted values before inserting them into
a command. Only expose arbitrary command execution to callers who should have
that access.

Each command runs in its own `sh` process. Shell state such as `cd` and exported
variables does not carry between commands. The snapshot must include POSIX `sh`.

## Observe the result

Both tables are private. Host views can read `ctx.db.daytona.sandbox` and
`ctx.db.daytona.execution`. Filter by the caller's owner key before returning rows.

- Sandbox state becomes `Ready` when Daytona reports that it has started.
- Only one unresolved command is allowed per sandbox.
- `Succeeded` means exit code zero. `Failed` means a nonzero exit code.
- `Unknown` means the result is not established. It does not mean the command failed.
- `deleteRequested` blocks new commands. Wait for `Deleted` before treating cleanup as complete.

For command output, use [Daytona's session logs](https://www.daytona.io/docs/en/process-code-execution/).

## Failure handling

A lost response can leave a request accepted by Daytona but unconfirmed locally.
The scheduled procedure checks the sandbox name or command session to recover
the result. It does not repeat create or command submissions with unknown outcomes.

An unresolved sandbox still counts toward its owner's limit. An unknown command
blocks new commands on that sandbox. Inspect it in Daytona before submitting a
replacement, or delete the sandbox to stop its work. If creation remains unknown
and no remote ID is available, the record stays pending until an operator
investigates. Execution across the two services has no exactly-once guarantee.

Deleting a sandbox stops its work and removes its files. It cannot reverse
external effects a command has already caused. Confirm cleanup in Daytona
before deleting the SpacetimeDB database.

After restoring a database backup, inspect the matching Daytona resources before
enabling its timer. Restoring local state does not restore remote commands.

## Limits and defaults

- Choose a finite lifetime of 1 to 60 minutes. Daytona enforces it independently
  of the application timer. The example uses 10 minutes.
- Sandboxes have private previews and blocked outbound network access. Select
  a prepared Linux snapshot with the tools the command needs.
- The default owner limit is two active or unresolved sandboxes. Deleted records
  do not consume this limit.
- Retain at most 100 sandbox records and 100 execution records per sandbox.
  Admission stops when these limits are reached. Resolved history is eligible
  for removal after 24 hours. Request deduplication ends with history removal.
- Commands are limited to 4 KiB. Owner and request keys are limited to 128 bytes.
- Each tick checks at most one sandbox and one execution. Provider requests have
  a 10-second HTTP timeout. Remote work can continue after an HTTP timeout.
- The default approved toolbox origin is `https://proxy.app.daytona.io`. If a
  region uses another origin, verify it with Daytona and set `toolboxOrigins` in
  trusted host configuration. Never accept this setting from a client.

Use a unique `namespace` for each installation in a database. Set sandbox size
through `snapshot`. Sandbox limits and expiry do not cap your Daytona bill.

## Development

```bash
pnpm test
pnpm typecheck
pnpm lint
pnpm build
```

Run `pnpm test:local` for database behavior, overlapping-worker, and crash-recovery
tests. Set `SPACETIME_BIN` to the matching CLI path if needed. The suite starts an
isolated server, tests against a simulated provider, and removes its temporary
data afterward. It does not create a paid Daytona sandbox.

Live provider verification is separate: follow the example, run `exit 0` and
`exit 7`, delete the sandbox, and confirm its removal in Daytona.

Licensed under Apache-2.0.
