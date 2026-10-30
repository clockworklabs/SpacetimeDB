# Daytona example

A Python playground. Create a temporary Daytona sandbox, run Python in it, and
read the output in the browser. Daytona runs the code; SpacetimeDB keeps track
of each sandbox, each run, and whether it succeeded.

## Run it locally

This example uses a real Daytona account. Creating a sandbox uses paid Daytona
credits. You need an API key that can manage sandboxes and run commands, and
access to the `daytona-small` snapshot, which must include `python3` and `sh`.

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

The module reads the API key from its environment. Set it in the terminal you
publish from; publishing stores it in the database. In PowerShell:

```powershell
$env:DAYTONA_API_KEY = '...'
```

In bash, use `export DAYTONA_API_KEY=...`.

From `modules/spacetime-daytona-ts/example`, run:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8816>. The page shows a `spacetime call` command. Run it
from the same CLI login you published with, to let that browser create
sandboxes.

## Try it

1. Click **Create sandbox** and wait for **Run** to appear.
2. Click **Run**. The output shows `4 orders`, `25 items`, and
   `Saved /tmp/report.txt`.
3. Open **Examples**, choose **Read report**, and click **Run**. The output
   shows the report saved by the first run: files stay in the sandbox between
   runs.
4. Edit the code and run it again. Try `raise ValueError("Try another input")`
   to see a failed run and Python's error.
5. Open **Sandbox**, click **Delete sandbox**, and confirm. Wait for `Deleted`,
   then check in Daytona that the sandbox is gone.

Each sandbox has no outbound network access and is deleted by Daytona after 10
minutes. Each browser can have one sandbox at a time and sees only its own.
Keep SpacetimeDB running until a deleted sandbox shows `Deleted`; the server
deletes it in Daytona.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts) mounts the submodule,
  sets the snapshot, lifetime, and per-user limit, checks who may use it, and
  defines the views.
- [src/app.ts](./src/app.ts) handles the controls and shows the output.
- [src/presets.ts](./src/presets.ts) holds the two example scripts.

After changing the server code, run `pnpm run build:module`. This keeps
existing sandboxes and the stored API key. Restart `pnpm run dev` after changing
the browser code.

To start over, delete your sandboxes first, then run
`pnpm run build:module:fresh`. **This deletes all data in the local
`spacetime-daytona-example` database.** Sandboxes still in Daytona are no longer
tracked; delete them in Daytona.

For instructions on adding sandboxes to your own app, see the
[Daytona package README](../README.md).

## Configuration

See [.env.example](./.env.example) to change the database address or web-server
port. The web server never reads the API key.

To replace the API key, set `DAYTONA_API_KEY` and run:

```bash
spacetime publish --server local --env-only spacetime-daytona-example
```

## Before deploying

The example only lets identities you have authorized run code, because anyone
who can run code spends your Daytona credits. Keep a check like that, and
connect it to your app's user accounts.

## Troubleshooting

- **`daytona.operator_only`:** run the `spacetime call` command shown on the
  page.
- **`daytona.http_401` or `daytona.http_403`:** check the API key and its
  permissions, then publish it again with `--env-only`.
- **The output didn't load:** click **Retry output**. This reads the output
  again without running the code again.
- **A sandbox or run shows `Unknown`:** Daytona may have done the work, but
  SpacetimeDB couldn't confirm it. Check the sandbox in Daytona before running
  the code again, or delete the sandbox.
- **`daytona.untrusted_toolbox`:** Daytona returned a command address the
  submodule doesn't trust. Check it with Daytona before adding it to
  `toolboxOrigins` in [spacetimedb/src/index.ts](./spacetimedb/src/index.ts).
