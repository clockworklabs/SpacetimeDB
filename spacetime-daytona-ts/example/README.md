# Daytona example

Create a temporary sandbox, run a shell command, and watch its state and exit code
update in the browser.

The example allows one sandbox per owner. It uses `daytona-small`, blocks outbound
network access, and sets a 10-minute expiry. Creating a sandbox can use paid Daytona
credits. For command output, use
[Daytona's session logs](https://www.daytona.io/docs/en/process-code-execution/).

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The workspace SDK and matching SpacetimeDB CLI and server from this checkout,
  with the CLI available as `spacetime`.
- A Daytona API key with sandbox management and execution access, and access to
  the `daytona-small` snapshot.

The example uses the unpublished submodule from this workspace.

## Start

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

Set `DAYTONA_API_KEY` in the shell that will publish the module. In PowerShell,
`$env:DAYTONA_API_KEY = 'your-key'` sets it for that shell. Do not commit the key or
put it in the browser environment.

In Bash:

```bash
export DAYTONA_API_KEY='your-key'
```

From this directory:

```bash
pnpm install
pnpm --filter spacetimedb build
pnpm --dir spacetimedb run publish:local
pnpm run build
pnpm dev
```

Open [the example](http://127.0.0.1:8816). The page shows a command for the database
owner to authorize that browser identity. Run it from the same CLI login used to
first publish the database. Each authorized browser can create its own sandbox
and sees only its own records.

1. Select **Create** and wait for `Ready`.
2. Enter `exit 0` and select **Run**. Expect `Succeeded` and exit code `0`.
3. Enter `exit 7` and select **Run**. Expect `Failed` and exit code `7`.
4. Select **Delete sandbox** and confirm. Wait for `Deleted`.
5. Confirm the resource is gone in Daytona.

Keep the server running until deletion is confirmed. Expiry also stops work and
removes sandbox files. Clean up remote sandboxes before deleting the database.

## Configuration

The web server listens only on `127.0.0.1`. It reads `PORT` (default `8816`),
`STDB_URI` (default `ws://127.0.0.1:3000`), and `SPACETIMEDB_DB_NAME` (default
`spacetime-daytona-example`) from its process environment. It does not read the
Daytona key.

To rotate the key, set `DAYTONA_API_KEY` in your shell and run:

```bash
spacetime publish --server local --env-only spacetime-daytona-example
```

Change the snapshot, lifetime, or owner limit in
[`spacetimedb/src/index.ts`](./spacetimedb/src/index.ts). See the
[submodule README](../README.md) to integrate it into your application.

## Errors

- `operator_only`: run the authorization command shown on the page.
- `http_401` or `http_403`: check the published key and its permissions.
- `untrusted_toolbox`: verify the returned proxy origin before changing the
  approved origins in host configuration.
- `Unknown`: inspect the remote resource. Do not submit a replacement command
  until you know what happened. The original command may have run.
