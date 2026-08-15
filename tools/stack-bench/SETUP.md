# What produced a number

Every figure this benchmark reports depends on a configuration. This is that
configuration, what is pinned, what is deliberately not, and where each value is
recorded so a result can be traced back to it.

Everything below lands in `results/<run>/run.json` under `setup`. If a value is
not in that file, it is not part of the record and a number that depends on it
cannot be defended.

## The model and how hard it thinks

| | value | where it comes from |
|---|---|---|
| model | `claude-sonnet-5` | `--model`, default in `bench.mjs` |
| reasoning effort | **`high`** | `--effort`, pinned in `agent.mjs`; `STACK_BENCH_EFFORT` overrides |
| thinking budget | **unset — CLI default** | deliberately not pinned; see below |
| reasoning produced | measured per run | `levels[].thinking` — blocks and signature bytes |

**Effort is pinned to `high`.** It was not pinned for most of this project's
life: the harness forwarded the whole environment and this machine carries
`CLAUDE_EFFORT=high`, so every earlier run took that value by accident. The
comparisons were still fair — both stacks got the same level — but nothing
recorded which level produced a number, and another machine would have produced
different ones silently. Pinned to `high` so results collected before and after
remain comparable.

**The thinking budget is deliberately NOT pinned.** Customers do not set
`MAX_THINKING_TOKENS`, so pinning it measures a configuration nobody runs. A pin
at 10000 was tried and changed nothing measurable. Instead each run records the
reasoning it actually produced, so a change in the CLI default shows up in the
data rather than being absorbed into every score.

## Cost-affecting settings

| | value | why it is pinned |
|---|---|---|
| prompt cache tier | `5m` (`FORCE_PROMPT_CACHING_5M=1`) | Cache reads are 97–98% of every bill. On the unpinned 1-hour tier a second run of the same backend reads a prefix the first paid to create and looks cheaper for reasons unrelated to the database. |
| auto-updater | disabled (`DISABLE_AUTOUPDATER=1`) | A CLI that updates itself mid-series changes the thing under test between one backend and the next. |
| permission mode | `acceptEdits` | Not `--dangerously-skip-permissions`, which is bypassPermissions and disables the deny rules entirely. |

## What is actually under test

Recorded per run, because a benchmark of SpacetimeDB that does not say which
SpacetimeDB is not reproducible:

- `setup.spacetime` — CLI version, commit and binary SHA-256 of the Linux binary
  built from this repo
- `setup.spacetimeBindings` — package version and source-tree SHA-256 of the
  local `file:` package
- `setup.database` — readable image reference and the database container's
  immutable image content ID
- `setup.cliVersion` — Claude Code version (the driver, not the subject)
- `setup.node` — separate orchestrator and coding-container Node versions
- `setup.platform`

## Guidance, and the asymmetry in it

`setup.skills` records which reference documents were inlined. SpacetimeDB
builds get `typescript-server` and `typescript-client`; PostgreSQL and MongoDB
get none, because their equivalent knowledge is already in the model's training
data.

**This is a real asymmetry, and it is smaller than it looks.** It makes the
SpacetimeDB prompt roughly 2.2x larger (42,551 bytes against 19,337 in one
measured pair), and prompt bytes are re-paid on every turn. Multiplied out:
about 5,800 tokens per turn over 103 turns is 0.60M tokens, roughly **18 cents**
at cache-read rates. Against a $4.00 gap that is **4.5% of the gap and 1.6% of
the bill**.

State the denominator whenever this is quoted. It was once written here as
"12%", which was 12% of the extra cache-read TOKENS — a denominator no reader
would assume, and about seven times the share of cost it actually represents.
The asymmetry is worth disclosing for fairness, not because it explains the
cost difference. It does not: the compiler errors do.

## Where the build runs

Builds are **container-only**. The container
(`container/Dockerfile`) exists because the
harness is not on its filesystem — a fix round once read the scenario file
defining the criteria it was failing and then ran the grader, using nothing but
`grep` and `node`. Denying those paths is a blocklist; absence is not.

| flag | effect |
|---|---|
| *(default)* | container or refuse; model sessions have no host execution path |
| `CLAUDE_CODE_OAUTH_TOKEN_FILE` | bill to the Claude subscription using a dedicated long-lived token file |
| `--api-key <key>`, `ANTHROPIC_API_KEY` | bill to a key instead of the plan credential |
| `STACK_BENCH_IMAGE` | image reference (default `stack-bench-build:2.1.226`) |

Build the image first, and — for SpacetimeDB — a Linux build of this
repository's CLI, since `target/release` holds a Windows binary a container
cannot execute:

```bash
docker build -t stack-bench-build:2.1.226 tools/stack-bench/container
```

```bash
bash tools/stack-bench/container/build-linux-cli.sh
```

Verify the model-free container lifecycle before spending a coding session:

```bash
npm run preflight -- --backend spacetime,postgres,mongodb --track ecommerce --levels 1-2 --smoke
npm run test:container
```

The first command is the operator admission check: exact requested scope,
Docker/Compose and architecture, CPU/memory/disk floors, clock, digest-matched
services, free run ports, credentials, provider-declared outbound access, Linux
CLI architecture, and persistent result-volume writes. `bench.mjs` repeats the
full no-model smoke automatically and refuses before any model call if a check
fails. Its typed `preflight.json` is attached to the run; secrets are never
included.

This starts a dedicated host on an ephemeral port, stages the repository SDK
with its runtime dependencies, runs `spacetime dev` inside the real build image,
publishes a fixture, checks it through SQL, verifies the integrated log stream
is authorized, and verifies cleanup. Log authorization is a hard assertion: a
publish-only success cannot make this smoke green.

The Dockerfile pins its Node base by manifest digest. PostgreSQL and MongoDB are
also pinned by manifest digest in `docker-compose.yaml`. At session start the
harness resolves `STACK_BENCH_IMAGE` to a `sha256:...` content ID and passes
that ID—not the possibly movable tag—to `docker run`.

`run.json` records `setup.isolation` (`mode`, readable `image`, immutable
`imageId`, and `hostAlias`) and
`setup.auth` (`subscription-token`, `api-key`, or `credentials`). Two numbers
are comparable only when the recorded image IDs and host-alias topology match.

**Isolation is pinned per run.** The first round writes `container` to
`.stack-bench-isolation` beside the app; later rounds require it. An app with
prior benchmark state and no valid pin is refused as ambiguous rather than
silently adopted.

If the image or Linux CLI is missing, the run refuses before model spend.
`run.json` reports the Linux CLI's own version, since the Windows and Linux
builds go stale independently. See `CONTAINER-DESIGN.md`.

The appliance defaults to subscription billing through a dedicated long-lived
setup-token file. It is mounted read-only and its value is absent from Docker
command arguments. The older `~/.claude/.credentials.json` path is still
supported as an explicit recovery mode and remains read-write because the CLI
rotates it. A build can read the selected subscription credential; that is an
unavoidable property of allowing the coding CLI to authenticate, so the runner
must contain no unrelated workloads or credentials.

## The environment

The removed host path forwarded `process.env`, which is how `CLAUDE_EFFORT`
influenced earlier runs before anyone noticed. A container inherits only
`DISABLE_AUTOUPDATER`, `FORCE_PROMPT_CACHING_5M`,
`MAX_THINKING_TOKENS` and the API key (when used) are passed in, which closes
this hole for containerised runs rather than only recording it. Ambient variables matching
`CLAUDE*`, `ANTHROPIC*`, `MAX_THINKING*`, `DISABLE_AUTOUPDATER` and
`FORCE_PROMPT*` are now recorded in `setup.env`, with anything key-shaped
redacted to its presence.

This records the problem rather than removing it. The child also inherits the
parent session's identifiers (`CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_ENTRYPOINT`
and similar). Whether any of those change model behaviour is untested; they are
in the record so the question can be answered rather than guessed at.

## Run parameters

Recorded at the top level of `run.json`: `track`, `backend`, `model`,
`guidance`/`stack` (prescribed or free), level, run index, and `--fix-rounds`.

Cost is broken out per phase — `buildCostUsd` and `fixCostUsd` — because a total
alone cannot distinguish an expensive first attempt from an expensive repair,
and those are different claims about a stack.

Every level keeps one `buildSession` and an ordered `fixSessions` array. Each
session records its prompt, selected skills, contract, track manifest, scenarios
and rubric hashes; fix sessions also hash the exact bug report they received.
`sessionTotals` aggregates cost, tokens, output tokens, usage classes, turns,
prompt bytes, reasoning volume and model duration across those sessions. The
run totals aggregate sessions, tokens, output tokens, turns and model duration
across every level; wall-clock `durationSec` remains separate.

## Comparing runs

Use `compare-runs.mjs`. A criterion the grader could not evaluate is subtracted
from that run's denominator, which is right per run and a trap across runs — it
is how one stack was scored out of 48 while another was scored out of 50 for the
same level. The tool scores every run on the intersection and reports the rest
separately.

## Contamination

`run.json` carries `contaminated` and the evidence behind it. That covers the
file tools. **Bash is ungoverned by design**; `leak-audit.mjs` is the control for
that channel and must be run separately — a run without it has only had half its
contamination question answered.

The audit still applies to containerised runs: their transcripts are mounted out
to the host folder it already looks in, and it uses `/app` as the boundary when
the transcript says the session ran there. Verified on a container run that read
both its own file and `/etc/hostname` — the first passed, the second was flagged.
