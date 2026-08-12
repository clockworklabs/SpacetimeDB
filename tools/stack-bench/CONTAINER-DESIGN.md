# Isolating the build

## Why

A SpacetimeDB L2 fix round read the scenario file that defines the criteria it
was failing, read `grade.mjs`, and ran the grader itself — 23 accesses, all
through Bash:

    grep -n -A5 -B5 "staff-link\|1d\|1e" .../scenarios/02-features.json
    cd .../grader && node grade.mjs --url http://localhost:6473 --level 2 --spec ...

The sandbox denies the file tools and permits Bash deliberately; `leak-audit` is
the control and it worked — the run was voided. But detection is not prevention,
and the run cost $12.66 and produced nothing.

Two cheaper ideas were considered and rejected:

- **Deny the harness paths to Bash.** Another blocklist. The agent did not use a
  clever bypass; it used `grep`, `sed` and `node`. Enumerating badness fails.
- **A restricted OS user.** Not possible here: this machine is not
  Administrator, so a second account cannot be created; and the CLI's auth lives
  in the invoking user's profile, so a different user is not authenticated.

The property we actually want is that the grader **is not on the filesystem the
build can see**. Not denied — absent.

## Verified feasible

Checked on this machine before designing:

| question | answer |
|---|---|
| Does a container see the repo by default? | No — `ls /d` fails; only mounts exist |
| Do bind mounts work from Git Bash? | Yes, with `MSYS_NO_PATHCONV=1` (paths are otherwise mangled to `C:/Program Files/Git/...`) |
| Can a container reach the host services? | Yes — SpacetimeDB answered 200 on `host.docker.internal:3210` |
| …even a listener bound to 127.0.0.1? | Yes — Docker Desktop proxies through to the host's loopback, so the lint server keeps its loopback bind. The obvious guess is the opposite; it was measured. On plain Linux Docker (`--add-host=host.docker.internal:host-gateway`) it would not, and `lint-server.mjs --host` exists for that |
| Does the CLI work from outside the repo? | Yes — same commit reported (`4aa1fda3`) |
| Does an app install the local bindings from a copy? | Yes — `spacetimedb@2.8.0` |

## What goes in the image

- Node (pinned to the version runs are measured on — today `v22.22.1`)
- The `claude` CLI, pinned by version, auto-update disabled
- `git`, `curl`, and the toolchain a build needs
- Nothing from `tools/stack-bench`

## Mounts

| path | mode | why |
|---|---|---|
| the run's app work dir | rw | the thing being built |
| `crates/bindings-typescript` | **ro** | the `file:` dependency under test |
| `target/release/spacetimedb-cli` | **ro** | the CLI under test |

`tools/stack-bench` is never mounted. The grader, scenarios, contracts and
prompts do not exist inside.

Mounting the two dependencies read-only keeps them the artifacts actually under
test — the alternative, copying them to a neutral path, introduces a staged copy
that can drift from the repo, and this project has already been burned twice by
stale artifacts (a stale CLI binary, and a stale `results/<run>/app` that
produced a retracted finding).

## Network

Builds reach the databases and the SpacetimeDB host on `host.docker.internal`
(3210, 6532, 6537). Dev servers start inside the container and must be reachable
by the grader, which stays on the host: publish the track's port window
(`-p 6473:6473 -p 6573:6573 …`, per backend and run index) so
`portsFor()` keeps working unchanged.

## How the harness uses it

The container is the only coding-session runtime. Earlier host measurements are
historical diagnostic evidence and are not comparable to hardened runs.

0. **The container outlives the build session.** This is the load-bearing
   decision and the first version got it wrong. `docker run --rm` ended the
   container when the coding session returned, taking the app's dev servers with
   it, and the grader that runs afterwards had nothing to talk to. A sweep died
   exactly there — *"reseed FAILED (server did not come back)"*, then *"ABORTED:
   could not reset database"* — after spending $9.46 and grading nothing.

   Serving the app from the host instead is not available: a container install
   produces `@esbuild/linux-x64` and `@rollup/rollup-linux-x64-gnu`, so a Windows
   host cannot execute the app's `node_modules` at all (measured).

   So the container is long-lived (`docker run -d --init … sleep infinity`) and
   each round is `docker exec`'d into it. The build starts its own dev servers
   exactly as it does on the host and they survive for the same reason: the
   process that owns them is still alive. `--init` gives a real PID 1, so the
   servers left behind are reaped rather than accumulating as zombies under
   `sleep`.

   The name is `stack-bench-<work-dir>`, derived from the run's timestamped work
   directory: unique per run, and reconstructible from the app path alone — which
   is all `restart-backend.sh` is given. Ports are published at create time only,
   which is the other reason the session cannot own the container's lifetime.

   `bench.mjs` removes it at the end of the run, and `stopServers` deliberately
   does **not** — that runs mid-run before a rollback grade that still needs the
   app up. `stopServers` also skips its port-killing entirely for a containerised
   run: the host side of a published port is held by Docker's own proxy, and
   killing that takes the daemon's port forwarding down. (It took Docker Desktop
   itself down once, mid-run.)

1. `agent.mjs` spawns `container/run-build.mjs` instead of the CLI, with the same
   CLI arguments and the prompt still on stdin. `<STDB_PACKAGE>` and `<STDB_BIN>`
   become `/deps/...`, and the app is named as `/app` — which also removes the
   repo-root disclosure the contaminated run followed.
2. Host services are rewritten to `host.docker.internal`: the database URL, the
   SpacetimeDB URI, and the lint shim. Dev-server ports are *not* rewritten —
   those servers start inside and are published back out, so the grader on the
   host still reaches them at `localhost`. Both directions verified: the host
   read a container server on a published 6573, and the container got 200 from
   `host.docker.internal:3210`.
3. Auth: `--api-key`/`ANTHROPIC_API_KEY` when supplied, otherwise the host
   credential is bind-mounted at `/root/.claude/.credentials.json` so runs bill
   to the plan. Only that one file is mounted, not `~/.claude`. A key is
   preferred when present because it keeps a rotating credential off the build's
   filesystem; the plan credential is the default because plan usage is the
   requirement.
4. Transcripts: the host folder `leak-audit --app` already looks in is mounted
   onto `/root/.claude/projects/-app`, so the audit trail survives `--rm` and
   `leak-audit.mjs`, `cost-ledger.mjs` and `thinkingVolume()` work with no
   argument changes. The host's whole `~/.claude/projects` is deliberately not
   mounted — it holds every other run's transcripts.
5. `leak-audit.mjs` uses `/app` as the boundary when the transcript says the
   session ran there. Without this every legitimate read of the app's own source
   resolves as an escape and each containerised run is voided for existing. The
   test is the exact literal `/app`, not "cwd outside the boundary": the looser
   rule could have adopted the harness directory as the boundary for the run
   that cd'd into it, and reported that run clean.
6. `reset-db.sh` still runs on the host. **`restart-backend.sh` does not** — the
   app tier now lives in the container, so the script re-runs its own logic
   inside via `docker exec`, piping itself in on **stdin** rather than being
   mounted, so no harness file ever appears on the filesystem the build can read.
   Only the app tier redirects; the `spacetime` branch restarts the SpacetimeDB
   host, which is a machine-level service and stays on the host.

   Two Windows/Linux boundary bugs had to be fixed to make that work, both of
   which fail silently rather than loudly:

   - **CRLF.** This repo checks the script out with CRLF (`git ls-files --eol`
     reports `i/lf w/crlf`). Git Bash tolerates it; the container's Linux bash
     reads `set -euo pipefail\r`, rejects `pipefail\r` as an option name, and
     dies on line 21 — so the restart did nothing and said nothing. The script is
     piped through `tr -d '\r'`.
   - **No `lsof`.** `kill-port` locates a listener with `lsof` on Linux, and
     `node:22-slim` ships none of `lsof`, `fuser`, `ss` or `netstat`. It printed
     *"Process on port N killed"*, exited 0, and left the server running. Every
     durability and deploy-window test would have passed without restarting
     anything — a pass that means nothing, which is worse than a failure.
     `lsof` is now in the image, **and** the script verifies the port is actually
     dead after the kill and exits 3 if it is not. A restart that cannot be
     proven is now a hard error.

## Verified end to end (2026-08-10)

Checked after the rewrite, with no LLM spend:

| behaviour | result |
|---|---|
| Dev server survives the build session exiting | host reached it after `docker exec` returned |
| `restart-backend.sh` restart | real restart — listener PID changed 107 → 186 |
| `restart-backend.sh` stop | port genuinely stops answering |
| `restart-backend.sh` start | comes back, reachable from the host |

Still outstanding: one cheap real build, end to end through reseed and grading,
before the container becomes the default again. Verifying the build step alone is
what produced the $9.46 loss.

## SpacetimeDB: the CLI has to be built for Linux

`target/release/spacetimedb-cli.exe` is a Windows PE binary (`MZ`), so mounting
it into a Linux container mounts something unrunnable. Substituting a
`spacetime` from the image was rejected — the benchmark publishes modules with
the CLI built from **this repository**, and a release build is different
software reported under the same name.

So the CLI is built for Linux from the same checkout:

```bash
bash tools/stack-bench/container/build-linux-cli.sh
```

It compiles `spacetimedb-cli` in a `rust:1.93-slim-bookworm` container (the
toolchain comes from `rust-toolchain.toml`, so the pin moving needs no edit
here) and writes `container/bin/spacetimedb-cli`. Cargo's target directory is a
named Docker volume rather than a path in the repo: a Rust build against a
Windows bind mount is far slower, and it would put a Linux binary next to the
Windows artifacts in `target/` where picking the wrong one is easy — this
project has already retracted a finding that came from a stale CLI.

Nothing else is needed inside the container. The TypeScript module build shells
out only to `tsc` from the module's own `node_modules`; the bundler (rolldown)
is linked into the CLI.

Two SpacetimeDB-specific mounts follow from this:

- `container/bin/spacetimedb-cli` at `/deps/spacetimedb-cli`, read-only.
- `<run>/.spacetime-cli-config` at `/root/.config/spacetime`. The CLI keeps the
  identity and token it mints on first publish there (`$XDG_CONFIG_HOME/spacetime`
  on Linux, per `crates/paths`). Under `--rm` that would be discarded every
  round, so a fix round would arrive as a different identity and be refused
  ownership of the module the build round published. On the host this config
  persists globally; persisting it per run reproduces that rather than inventing
  it. It sits beside the app, not inside it — the app directory is what gets
  copied into `source/` and audited, and a token belongs in neither.

`run.json` records the version and binary SHA-256 of the **Linux** CLI, because
the Windows and Linux builds go stale independently. It also records the
repository SDK source hash, the image's Node and Claude Code versions, and the
immutable image content ID Docker actually executes.

If the Linux CLI is missing or is not an ELF binary, the run refuses before a
model session starts. There is no host fallback and therefore no second
lifecycle or filesystem boundary to audit.

## Cost, honestly

- It re-baselines: different filesystem, and the image pins CLI 2.1.226 against
  the host's 2.1.222, so numbers taken before and after are not strictly
  comparable. The Dockerfile base and database services are pinned by manifest
  digest. At runtime the build tag is resolved once and Docker executes its
  immutable content ID. `run.json` records that ID and the image's own CLI and
  Node versions so two different images are never silently mixed.
- Per-run overhead is a container start and a dependency install; the install
  already happens.
- The build can still read the credential mounted into it. That is a real
  residual risk, accepted for now because the alternative is not running on the
  plan.

## What it does not fix

A build can still read its own environment, and anything else mounted into it.
Isolation bounds what is reachable; it does not make the build incurious. The
leak audit stays, and stays as a hard failure.
