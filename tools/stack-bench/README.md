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

`mode.unchangedFailureLimit` controls early stopping independently of the repair budget.
The initial failure counts as one observation. To allow all five repairs per feature,
set this limit to at least `6`; a limit of `3` can stop after two unchanged repairs.

Rejecting a repair restores its accepted source and recreates the isolated grading
database before restarting the app. This reset is between candidates, not inside
durability probes. A failed rollback preserves the accepted source, grade evidence,
and repair costs, and stops the attempt as a harness failure.

Between isolated ecommerce scenarios, every stack runs its normal application
startup after the database reset. This includes SpacetimeDB apps that perform
initialization outside module `init`. The harness does not guess migration names.
The shared agent request states that startup must initialize the supplied data and
accounts in an empty database, including after upgrades and repairs. Startup must
preserve current quantities, prices, and user data when a database already exists.
Earlier runs without this requirement need an audit if initialization failures
affected their scores; the updated request does not validate those scores retroactively.

New dependency plans retain previously disclosed interface contracts in upgrade
prompts by default. Set `mode.retainPriorContracts` to `false` to opt out.
Set `repair.budget.total` to `0` for a study with no repairs. Feature
work stays incremental; grading rules and repair budgets stay separate. See the
[prompting method](docs/prompting.md#dependency-progression).

Set a condition's `guidanceProfile` to `neutral-dev` to request the
`spacetime dev` watch workflow for SpacetimeDB. This opt-in profile reuses neutral
product guidance and adds a pinned workflow skill; other stacks are unchanged.
It does not change grading or repair policy. The default remains `neutral`.
The `neutral`, `neutral-dev`, and `neutral-managed-dev` profiles retain the selected
TypeScript server, TypeScript client, and CLI skills. Dev guidance adds a workflow; it does not replace the SDK
references or start a watcher by itself.

For a skill ablation, `neutral-dev-no-sdk` keeps the same backend document and
dev workflow but omits the TypeScript server, client, and CLI reference skills.
Label this condition separately from standard guidance in comparisons.

`neutral-managed-dev` instead supplies `/deps/spacetime-dev start|status|stop`.
The agent creates its project configuration, then starts the managed watcher.
The helper serializes starts, reports initial readiness, and keeps a log in the
agent home. It runs as the agent user, so normal container cleanup stops it.
It supports one assigned database and TypeScript binding targets inside `/app`.
Use a controller and coding image built with this support. This profile has a
separate guidance identity; it does not change grading or repair policy.

### Pause before a later depth

For a planned staged run, select the full target (for example, `levels: [1, 2, 3]`)
and set `mode.pauseAfterDepth: 2` with progressive dependency work. Each eligible
attempt waits after its L2 work, before the L3 request. Use enough parallelism for
the whole cohort: waiting attempts retain their processes and resource leases.

Inside the same appliance release, use:

```sh
node dist/commands/campaign-cli.js pause-status /path/to/campaign
node dist/commands/campaign-cli.js continue-depth /path/to/campaign
```

The release command waits for the cohort boundary: every attempt must be waiting
or terminal. Failed attempts stay in the cohort. Releasing the boundary does not
require 100% completion, grant repairs, restart earlier work, or change eligibility.
The existing progression rules determine which L3 features can start.

This keeps the same process, accepted source, live database, progression history,
model configuration, and cumulative cost and repair budgets. Source and progression
changes during the hold cause an error. `depth-pause.json` records each hold and
`depth-release.json` records the cohort release. Working-time allowance excludes the
planned wait; total wall duration and paused duration remain in the evidence.
Cancellation still works. Keep the controller running throughout the hold.
A controller loss is an interruption. `continue-depth` refuses a dead owner;
it cannot restore the database or agent session after controller shutdown.
Keep the full campaign directory for review; the partial research export omits
the control receipts.

This is a planned staged experiment, not a promise of identical model output or
wall-clock behavior. Database timers and external services can advance during a
hold. Cache expiry, provider changes, and changing host load can affect cost and
duration. Keep those limits in the study protocol and verify staged versus continuous
behavior before claiming equivalence. No checkpoint feature can retroactively turn
an already-started L2-only campaign into a predeclared L3 study. `campaign extend`
remains a separate source-seeded study.

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
Paid adapters share the coding runner and cost controls. Select Claude, OpenAI,
or OpenRouter through the [appliance credential guide](appliance/README.md#openai-credentials).
Current grading profiles are provisional until their qualification gates pass.

The dashboard refreshes visible running campaigns every five seconds and follows
saved evidence events. For single-execution Claude Code attempts, `~$` marks a
live estimate from completed response usage at the plan's pinned rates. Final
receipts replace that estimate. Unsupported or incomplete usage keeps the saved
cost visible. Live estimates do not enter scores, reports, or budget enforcement.
Planned depth holds show their paused state; elapsed time includes those holds.

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
  A repair candidate is accepted only under the mode's regression rules;
  rejected source remains available as evidence.
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

### Adding a coding agent

Register the agent in `src/agents/agent-adapters.ts`. Claude and Codex use the
same `commands/agent.ts` prompt and grading path. Their CLI arguments, process
handling, and result parsing live in `container/coding-providers.ts`; their
trusted API forwarding and usage parsing live in `container/broker-protocols.ts`.
Add a provider there when its protocol differs. Keep authentication in
`container/container-auth.ts`, outside the coding container.

A new provider must supply normalized token usage, preserve its tool transcript
for the shared audit, and enforce the plan's cost bound. Test it with a local
mock upstream before a paid run. Do not copy the container runner or add
provider conditions to prompts, grading, or campaign scheduling.
