# StackBench

StackBench measures how well coding agents deliver production-quality features
by default. Each agent builds an online store from a feature list on one stack,
and StackBench scores how many of its features hold up under production
conditions. Every run happens in an isolated Docker sandbox, every check is
scripted, and the evidence behind every result is kept.

The demo needs only Git and Docker. From the repository root:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

[Getting started](GETTING-STARTED.md) explains the demo and the first model run.

## What it covers

- **Stacks:** PostgreSQL, MongoDB, Convex, Supabase, and SpacetimeDB.
- **Agents:** Claude Code, Codex, Grok Build, Antigravity, and any model on
  OpenRouter.
- **Workload:** the [ecommerce track](tracks/ecommerce/LEVELS.md), a store with
  a warehouse, built feature by feature through a feature graph.

## How a run works

1. A plan fixes everything about the run; see [plans](appliance/README.md#plans).
2. The agent builds the store in a sandbox from the
   [prompt](docs/prompting.md).
3. StackBench restarts the store from its saved code, empties the database, and
   runs every check: pages in a real browser, direct server requests, database
   writes, restarts, and crashes.
4. [Grading](docs/grading.md) turns the check results into a score, with cost
   beside it.

## Documentation

- [Getting started](GETTING-STARTED.md), then the
  [appliance guide](appliance/README.md) to run campaigns.
- [Skills](skills/): procedures to run, recover, archive, qualify, and release.
- The [documentation index](docs/README.md) for how it works and how to change it.
