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
  can stop while unrelated branches continue. `workSelection` controls whether
  the agent gets one ready feature, all ready features, or the full graph. The
  manifest's `repair` object targets one failed feature or all current
  failures and sets the repair budget.

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
- `conditions/` owns guidance and repair feedback.
- `backends/` owns stack material sent to the coding agent.
- `src/stacks/` owns runtime stack adapters.
- `commands/` and `src/` own the CLI and reusable benchmark logic.
- `grader/`, `linter/`, and `reference-apps/` own validation.
- `appliance/` owns deployment. `dashboard/` is an optional interface.

Prompt selection and scoring selection stay separate. A behavior can be measured
without being named in the product request.

## Words

One word per thing. Every surface, from the CLI to the dashboard to the
artifacts, uses these.

- **campaign**: one comparison job. A plan file fixes the product, the
  stacks, the model, the checks, the budgets, and the repetitions; a result
  directory holds everything it produced.
- **attempt**: one stack building the product once inside a campaign. A
  campaign with three stacks and one repetition has three attempts.
- **execution**: one process run of an attempt. A retried attempt has two.
- **session**: one conversation with the coding agent. A build session
  writes the app; a repair session reacts to a failure report.
- **stack**: the technology under test, such as SpacetimeDB, PostgreSQL, or
  MongoDB. Flags still spell it `--backend`; the word is stack.
- **level**: one rung of a sequential campaign (L1, L2). **depth**: how far
  down the feature graph a dependency campaign has reached. They share a
  field but never a meaning.
- **feature**: one node of the dependency graph, the unit the agent builds
  and the grader scores. A feature opens when its parents pass.
- **questline**: a named path of features through the graph, such as
  identity or fulfilment. One questline can stop while the others continue.
- **check**: one scored criterion with a stable id such as `601b`. A
  **gate** check must pass before the feature's descendants open; a
  **guarantee** check costs points but never blocks.
- **disclosure**: whether a specification is requested (in the prompt),
  expected (not in the prompt, scored), or observed (not in the prompt, not
  scored).
- **first build**: the score before any repair. **repair**: one paid
  session that reacts to a failure report, plus the regrade after it.
  A repair that loses ground is rolled back.
- **passed** and **failed** are measured outcomes; failed is the
  application's fault. **inconclusive** means the harness could not
  measure the check: no credit, no blame, and the reason is recorded.
- **harness failure** and **provider failure** mean the benchmark or the
  model provider broke; the attempt is **excluded** from comparison data,
  as is a **contaminated** attempt whose agent read grading material.
- **needs attention**: a campaign that stopped and needs a person.
- **preflight**: the verifications before an attempt, each a **probe**
  such as `registry.cache`. A **smoke** preflight starts a real coding
  container without a model. **admission** is the record that preflight and
  policy allowed the campaign to start.
- **coding container**: the container the agent works in, created from the
  **build image**. It sees the app, its stack material, and nothing else.
- **clean source**: the accepted application source with nothing the
  agent's process left behind. Every grade starts the app from clean source.
- **credential broker**: the local proxy that holds the provider key so the
  coding container never sees it. Its **cost receipt** is the proof of what
  a session spent.
- **lease**: the record of which containers, ports, database, and locks an
  attempt owns, so cleanup and recovery act only on those.
