# Stack Bench

Stack Bench compares how coding agents build the same application with different
technology stacks. It runs each attempt in an isolated container, tests real
behavior, allows bounded repairs, and keeps the evidence behind every result.

## What it does

1. Compiles a versioned campaign that fixes the product request, model, stacks,
   checks, budgets, repetitions, and parallelism.
2. Verifies the runner before model work starts.
3. Gives the coding agent only the current product work and selected stack
   material. The agent cannot see the grader, checks, scores, or comparison data.
4. Grades the running app through separate browser sessions and stack operations.
5. Returns conclusive app failures for repair within the campaign budget.
6. Records score, cost, duration, source identity, and supporting evidence.

Only compatible, verified attempts become comparison data. Provider failures,
harness failures, and incomplete measurements remain separate.

## Run modes

- **Sequential:** complete each selected level before starting the next. Earlier
  checks run again to catch regressions.
- **Dependency:** each feature opens after its required parents pass. One branch
  can stop while unrelated branches continue. Repairs target one failed feature
  by default, or all current failures when `repairSelection` is `batch`. Set
  `workSelection` to `all-at-once` to request and grade every selected feature
  in one build.

## Start here

The supported v1 deployment is the Docker appliance on a dedicated Linux/amd64
runner. The controller has root-equivalent access through the Docker socket, so
do not run it beside unrelated workloads or credentials.

1. Follow the [appliance guide](appliance/README.md) to configure and run a
   campaign.
2. Follow the [development guide](docs/development.md) to work from source.
3. Use the [documentation index](docs/README.md) for architecture, grading,
   recovery, release, and track authoring.

Paid and subscription-backed model work runs only through the appliance. Local
source commands accept non-billable adapters for development and qualification.

## Ownership

- `tracks/` owns product requests, feature definitions, checks, and scenarios.
- `conditions/` owns guidance and repair policy.
- `backends/` owns stack material sent to the coding agent.
- `src/stacks/` owns runtime stack adapters.
- `commands/` and `src/` own the CLI and reusable benchmark logic.
- `grader/`, `linter/`, and `reference-apps/` own validation.
- `appliance/` owns deployment. `dashboard/` is an optional interface.

Prompt selection and scoring selection stay separate. A behavior can be measured
without being named in the product request.
