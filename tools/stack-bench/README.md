# Stack Bench

Stack Bench compares how coding agents build the same application with different
technology stacks. It runs each attempt in an isolated container, tests real
behavior, supports optional bounded repairs, and keeps the evidence behind every result.

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

Only compatible attempts with validated evidence become comparison data. Provider failures,
harness failures, and incomplete measurements remain separate.

## Run modes

- **Sequential:** complete each selected level before starting the next. Earlier
  checks run again to catch regressions.
- **Dependency:** each feature opens after its required parents pass. One branch
  can stop while unrelated branches continue. `workSelection` controls whether
  the agent gets one ready feature, all ready features, or the full graph. The
  manifest's `repair` object targets one failed feature or all current
  failures and sets the repair budget.

New dependency plans retain previously disclosed interface contracts in upgrade
prompts by default. Set `mode.retainPriorContracts` to `false` to opt out.
Set `repair.budget.total` to `0` for a study with no repairs. Feature
work stays incremental; grading rules and repair budgets stay separate. See the
[prompting method](docs/prompting.md#dependency-progression).

## Start here

From a clean checkout of the delivered branch, with Docker running, use one
command from the repository root:

```sh
docker compose -f tools/stack-bench/appliance/demo.compose.yaml run --build --rm demo
```

When the dashboard is ready, open [localhost:7331](http://localhost:7331).
The demo runs a model-free example across three stacks and shows its results.
It needs no provider credentials and makes no model calls. The first source
build can take substantial time. The dashboard stays running after the example finishes.

The demo uses the Docker appliance with Linux/amd64 containers. The host needs
Git and Docker with Compose and BuildKit. See the appliance guide for resource
requirements and the release checks that still need proof. The trusted
controller manages Docker through its socket; coding agents do not receive it.

1. Follow the [appliance guide](appliance/README.md) for requirements, results,
   and paid campaigns.
2. Follow the [development guide](docs/development.md) to work from source.
3. Use the [documentation index](docs/README.md) for architecture, grading,
   recovery, release, and track authoring.
4. Use the [authoring guide](docs/authoring.md) to add product work and checks.
   Review the [grading coverage map](docs/grading-coverage.md) before making
   claims about verified results.

Campaign execution and paid or subscription-backed model work run through the
Linux appliance. Portable source checks remain available for development.
Current grading profiles are provisional until their qualification gates pass.

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
