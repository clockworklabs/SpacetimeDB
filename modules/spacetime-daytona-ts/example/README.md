# Daytona example

Run Python in a temporary Daytona sandbox and read its output in the browser.
Build a report from sample CSV orders, then read the saved file in a second run.

The example allows one sandbox per owner. It uses `daytona-small`, blocks outbound
network access, and sets a 10-minute expiry. Creating a sandbox can use paid Daytona
credits. Output loads when the code finishes. The page shows the latest output,
up to 16,384 characters, including errors.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The workspace SDK and matching SpacetimeDB CLI and server from this checkout,
  with the CLI available as `spacetime`.
- A Daytona API key with sandbox management and execution access, and access to
  the `daytona-small` snapshot. The snapshot must contain `python3` and POSIX `sh`.

The example uses the submodule and SDK from this workspace.

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

1. Select **Create sandbox** and wait for **Run** to become available.
2. Select **Run**. Expect `4 orders`, `25 items`, and
   `Saved /tmp/report.txt` in the output.
3. Open **Examples**, select **Read report**, then **Run**. Expect `4 orders` and
   `25 items` read from the saved report. Files remain between runs in the same
   sandbox. Loading a sample only changes the editor.
4. Edit the code and run it again. Try `raise ValueError("Try another input")`
   to see Python's error output and a failed run.
5. Open **Sandbox**, select **Delete sandbox**, and confirm. Wait for `Deleted`,
   then confirm the resource is gone in Daytona. Output cannot be fetched after
   deletion.

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
Edit the starter scripts in [`src/presets.ts`](./src/presets.ts).

## Errors

- If fetching output fails, select **Retry output**. This fetches the output
  without running the code again.
- `operator_only`: run the authorization command shown on the page.
- `http_401` or `http_403`: check the published key and its permissions.
- `untrusted_toolbox`: verify the returned proxy origin before changing the
  approved origins in host configuration.
- `Unknown`: inspect the remote resource. Do not submit a replacement command
  until you know what happened. The original command may have run.
