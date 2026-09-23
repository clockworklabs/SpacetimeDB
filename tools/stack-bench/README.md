# Stack Bench

Stack Bench compares how coding agents build the same application with different
technology stacks. It runs each attempt in an isolated container, tests real
behavior, supports optional bounded repairs, and keeps the evidence behind every result.

**New here? [Start your first run](GETTING-STARTED.md).** The model-free demo needs
only Git and Docker:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

Then open [localhost:7331](http://localhost:7331).

## What it does

1. Compiles a versioned campaign that fixes the product request, model, stacks,
   checks, budgets, repetitions, and parallelism.
2. Verifies the runner before model work starts.
3. Gives the coding agent only the current product work and selected stack
   material. The agent cannot see the grader, checks, scores, or comparison data.
4. Grades the running app through separate browser sessions and stack operations.
5. When repairs are enabled, returns conclusive app failures within the declared budget.
6. Records check completion, token usage, cost, duration, source identity, and
   supporting evidence. Weighted scores remain separate from completion.

Only compatible attempts with validated evidence become comparison data. Provider
failures, harness failures, and incomplete measurements remain visible but separate.
Results are provisional until the selected checks have current
[qualification](reference-apps/README.md).

## Scope

- **Stacks:** SpacetimeDB, PostgreSQL, and MongoDB. Self-hosted Convex supports
  the ecommerce dependency L1–L3 path.
- **Tracks:** [ecommerce](tracks/ecommerce/LEVELS.md) (storefront and warehouse)
  and [chat](tracks/chat/LEVELS.md).
- **Modes:** sequential levels, where each level must pass before the next, or a
  dependency graph, where each feature opens once its parents pass.
- **Agents:** Claude Code, Codex, and OpenRouter, through the
  [appliance](appliance/README.md#provider-credentials).

Campaign execution runs in the Linux Docker appliance. Source checks also run
on a development machine.

## Documentation

Start with [Getting started](GETTING-STARTED.md), then the
[appliance guide](appliance/README.md) for campaigns. Everything else is in the
[documentation index](docs/README.md).

## Layout

- `tracks/` owns product requests, feature definitions, checks, and scenarios.
- `conditions/` owns guidance and repair feedback.
- `backends/` owns stack material sent to the coding agent.
- `src/stacks/` owns runtime stack adapters.
- `commands/` and `src/` own the CLI and reusable benchmark logic.
- `grader/`, `linter/`, and `reference-apps/` own validation.
- Retained qualification records go in `qualification-evidence/`, cited by path
  from each calibration. None are retained while qualification is pending.
- `appliance/` owns deployment. `dashboard/` is an optional interface.

Prompt selection and scoring selection stay separate. A behavior can be measured
without being named in the product request.
