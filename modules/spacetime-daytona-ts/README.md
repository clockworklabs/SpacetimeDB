# @spacetimedb/daytona

Let users run commands in temporary Daytona sandboxes from your SpacetimeDB
application. Daytona provides the Linux machine. SpacetimeDB keeps track of who
owns each sandbox, which commands ran, and whether they succeeded.

Each request is saved in the database first, and a scheduled procedure sends it
to Daytona. If the server restarts or a request to Daytona fails, the submodule
checks Daytona later instead of sending the request twice. Your app decides who
can create sandboxes and run commands.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/daytona
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

You also need a Daytona API key that can manage sandboxes and run commands in
them. Creating a sandbox uses paid Daytona credits.

## Integrate into an application

This module lets each allowed user create a sandbox, run commands in it, read
the output, and delete it:

```ts
import { ScheduleAt } from 'spacetimedb';
import {
  schema,
  SenderError,
  t,
  table,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as daytona from '@spacetimedb/daytona/submodule';
import { client, daytonaTick, sandbox } from '@spacetimedb/daytona';

// Identities allowed to run code. Each run spends Daytona credits.
const allowed = table(
  { name: 'allowed' },
  { identity: t.identity().primaryKey() }
);

const spacetimedb = schema(
  { daytona, daytonaTick, allowed },
  {
    env: { DAYTONA_API_KEY: t.string() },
  }
);
export default spacetimedb;

type Tx = TransactionCtx<InferSchema<typeof spacetimedb>>;

const sandboxes = client({
  submodule: (tx: Tx) => tx.as.daytona,
  namespace: 'daytona',
  snapshot: 'daytona-small',
  ttlMinutes: 10,
});

// Each allowed SpacetimeDB identity owns its own sandboxes.
const owner = (tx: Tx) => {
  if (!tx.db.allowed.identity.find(tx.sender))
    throw new SenderError('not_allowed');
  return tx.sender.toHexString();
};

export const init = spacetimedb.init(tx => {
  tx.db.allowed.insert({ identity: tx.sender });
  tx.db.daytonaTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(3_000_000n), // every 3 seconds
  });
});

export const maintain = spacetimedb.procedure(
  { onSchedule: daytonaTick },
  { tick: daytonaTick.rowType },
  t.unit(),
  ctx => {
    sandboxes.reconcile(ctx, ctx.env.DAYTONA_API_KEY);
    return {};
  }
);

export const createSandbox = spacetimedb.reducer(
  { requestKey: t.string() },
  (tx, { requestKey }) => {
    sandboxes.createSandbox(tx, { owner: owner(tx), requestKey });
  }
);

export const runCommand = spacetimedb.reducer(
  { sandboxId: t.u64(), requestKey: t.string(), command: t.string() },
  (tx, args) => {
    sandboxes.runCommand(tx, { owner: owner(tx), ...args });
  }
);

export const getCommandOutput = spacetimedb.procedure(
  { executionId: t.u64() },
  t.object('CommandOutput', { output: t.string(), truncated: t.bool() }),
  (ctx, { executionId }) =>
    sandboxes.getCommandOutput(ctx, ctx.env.DAYTONA_API_KEY, {
      owner: ctx.withTx(owner),
      executionId,
    })
);

export const deleteSandbox = spacetimedb.reducer(
  { sandboxId: t.u64() },
  (tx, args) => {
    sandboxes.deleteSandbox(tx, { owner: owner(tx), ...args });
  }
);

export const mySandboxes = spacetimedb.view(
  { public: true },
  t.array(sandbox.rowType),
  ctx => [...ctx.db.daytona.sandbox.owner.filter(ctx.sender.toHexString())]
);
```

Publish with the API key in your shell. It is stored privately in the database:

```bash
DAYTONA_API_KEY='...' spacetime publish my-app
```

To replace the key later, set the new value and run
`spacetime publish my-app --env-only`. Use a key from the same Daytona
organization, or the submodule can't find the sandboxes it already created.

`init` only runs when a database is first created. To add the submodule to an
existing database, run the two inserts inside `init` once, from a reducer only
your admins can call.

The browser uses a sandbox in four steps:

1. Call `createSandbox` and wait for the sandbox's state to become `Ready`.
2. Call `runCommand`. Use a new request key for each command.
3. Wait for the command's state to become `Succeeded` or `Failed`.
4. Call `getCommandOutput` to read what it printed.

The submodule's tables are private. The view above lists the caller's
sandboxes; add a similar view over `ctx.db.daytona.execution` for commands. The
[browser example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-daytona-ts/example)
has both views and the browser code.

Anyone who can call `createSandbox` and `runCommand` can run any code in a
sandbox and spend your Daytona credits, so `owner` only accepts identities in
the `allowed` table. `init` adds the identity that publishes the database; add
others from a reducer only that identity can call, as the
[example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-daytona-ts/example/spacetimedb/src/index.ts)
does with `grantAccess`. Always set the owner on the server. Never take it from
the browser, and never send the API key to a client.

## API

`client(options)` takes:

- `submodule`: returns the mounted submodule from a transaction.
- `namespace`: a short name for this installation, made of lowercase letters,
  digits, and `-`, starting with a letter, at most 16 characters. Use a
  different one for each installation in the same database.
- `snapshot`: the Daytona snapshot each sandbox starts from. It sets the
  sandbox's size and installed tools, and must include POSIX `sh`. Sandboxes
  have no outbound network access, so install what commands need in the
  snapshot.
- `ttlMinutes`: how long a sandbox lives, 1 to 60. Daytona deletes it after
  that even if your server is down.
- `maxSandboxesPerOwner`: 2 by default. A sandbox counts until it is `Deleted`.
- `toolboxOrigins`: the HTTPS origins allowed to receive the API key when
  running commands. The default is `https://proxy.app.daytona.io`. Change it
  only after checking the new origin with Daytona, and never from client input.

Invalid options fail module loading with `daytona.invalid_config`.

It returns:

- `createSandbox(tx, { owner, requestKey })` records a new sandbox and returns
  its ID. Calling it again with the same request key returns the same sandbox.
- `runCommand(tx, { owner, sandboxId, requestKey, command })` records a command
  and returns its ID. The sandbox must be `Ready` with no command still
  running. Calling it again with the same request key and command returns the
  same command.
- `getCommandOutput(ctx, apiKey, { owner, executionId })` returns
  `{ output, truncated }` for a finished command, read from Daytona. Output is
  limited to 16,384 characters and is not stored in SpacetimeDB, so it is gone
  once the sandbox is deleted.
- `deleteSandbox(tx, { owner, sandboxId })` stops new commands at once;
  `reconcile` deletes the sandbox in Daytona.
- `reconcile(ctx, apiKey)` sends queued requests to Daytona and checks on ones
  that were interrupted. Run it on a schedule. Each run handles at most one
  sandbox and one command.

A sandbox's `state` is `Queued`, `Creating`, `Ready`, `Unavailable`, `Unknown`,
`Deleting`, or `Deleted`. A command's `state` is `Queued`, `Submitting`,
`Running`, `Succeeded`, `Failed`, or `Unknown`. `Succeeded` means exit code 0.
Both keep the latest error code in `error`.

`Unknown` means Daytona may have done the work but the submodule could not
confirm it. The submodule does not repeat the request. Check the sandbox in
Daytona, or delete it. An `Unknown` command blocks new commands in its sandbox,
and an `Unknown` sandbox still counts toward its owner's limit.

Each command runs in its own `sh` process, so `cd` and exported variables do not
carry over to the next command. Commands are at most 4 KiB, and request keys at
most 128 bytes. Quote untrusted values before putting them in a command.

The submodule keeps at most 100 sandboxes and 100 commands per sandbox. Finished
commands and deleted sandboxes are removed after 24 hours; after that, a reused
request key creates a new one.

Errors reach the caller as `daytona.*` codes: `invalid_request_key`,
`invalid_command`, `request_key_conflict`, `sandbox_limit`, `history_full`,
`sandbox_not_found`, `sandbox_not_ready`, `command_pending`,
`execution_not_found`, `output_not_ready`, and `scheduler_only`. A failed
Daytona request reports `daytona.http_<status>`, such as `daytona.http_401` for
a bad key, or `daytona.transport`, `daytona.invalid_response`,
`daytona.untrusted_toolbox`, or `daytona.ownership_mismatch`.

Deleting a sandbox removes its files but cannot undo what a command already did
outside it. Delete sandboxes in Daytona before deleting the database. After
restoring a database backup, check the matching sandboxes in Daytona before
starting the schedule again.

Package entrypoints:

- `@spacetimedb/daytona/submodule` supplies the submodule's tables.
- `@spacetimedb/daytona` exports `client`, `daytonaTick`, `sandbox`,
  `execution`, and their types.

## Testing

From `modules/spacetime-daytona-ts`:

```bash
pnpm test
pnpm test:local
```

`test:local` needs the SpacetimeDB CLI on `PATH`, or `SPACETIME_BIN` set to it,
and no Daytona account. It runs a real server and a fake Daytona that fails on
purpose, kills the server while a command is running, and checks that every
sandbox and command ends up right.

## License

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
