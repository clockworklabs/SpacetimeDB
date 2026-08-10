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

## What has to change in the harness

1. `agent.mjs` spawns the CLI with `cwd: args.app`. It would instead spawn
   `docker run` with the mounts above and the prompt on stdin. The prompt's
   `<STDB_PACKAGE>` and `<STDB_BIN>` become container paths, which also removes
   the repo-root disclosure that the contaminated run followed.
2. Auth: the CLI needs credentials inside the container. Mounting the host
   `~/.claude` would re-expose the host filesystem shape and the transcripts, so
   it should be a token passed by environment, scoped to the run.
3. `reset-db.sh` and `restart-backend.sh` run on the host and address the app by
   path; the app's files remain visible on the host through the same work dir,
   so these keep working, but each needs checking rather than assuming.
4. Transcripts are written inside the container and must land somewhere the
   host can archive — a mounted transcript directory, kept out of the app dir so
   a build cannot read its own audit trail.

## Cost, honestly

- Auth forwarding is the main risk and the main unknown.
- It re-baselines: different filesystem, possibly different Node, so L1 and L2
  numbers taken today are not strictly comparable to numbers taken after. It
  should therefore land with a planned sweep, not between two halves of one.
- Per-run overhead is a container start and a dependency install; the install
  already happens.

## What it does not fix

A build can still read its own environment, and anything else mounted into it.
Isolation bounds what is reachable; it does not make the build incurious. The
leak audit stays, and stays as a hard failure.
