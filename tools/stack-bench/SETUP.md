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

- `setup.spacetime` — CLI version and commit of the binary built from this repo
- `setup.spacetimeBindings` — e.g. `spacetimedb@2.8.0`, the local `file:` package
- `setup.database` — container image, e.g. `postgres:16`, `mongo:7`
- `setup.cliVersion` — Claude Code version (the driver, not the subject)
- `setup.node`, `setup.platform`

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

## The environment

`agent.mjs` forwards `process.env` to the build. That is how `CLAUDE_EFFORT`
influenced every run before anyone noticed. Ambient variables matching
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
