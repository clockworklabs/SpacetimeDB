# Stack Bench system design

Stack Bench turns one versioned test plan into traceable comparison evidence. The
system must make every decision, action, result, and cost traceable without
using chat history or operator memory.

## One owner for each fact

| Layer | Owns | Durable output |
|---|---|---|
| Definitions | Product work, prompt modules, checks, stacks, models, and budgets | Versioned source files |
| Compiler | The exact work matrix and all bound identities | `plan.json` |
| Job store and worker | Immutable submission, host placement, credential references, and exclusive execution claim | Job and claim records |
| Admission | Whether the exact plan can run on this appliance | Admission artifact |
| Scheduler | Attempt order, concurrency, continuations, and terminal state | `state.json` |
| Run engine | Build, grade, repair, resource ownership, and cleanup | Attempt directory |
| Grader | Typed check results and evidence | Grade artifacts |
| Progression engine | Open, passed, failed, and blocked features | `progression-state.json` |
| Report | A reproducible view of retained evidence | `report.json` and `report.html` |

No layer can silently replace a decision from a layer above it. A view can
summarize durable data, but it cannot create new run state.

## Terms

One word per thing. The CLI, dashboard, and artifacts use these.

- **campaign**: one comparison job. A plan file fixes the product, the stacks,
  the model, the checks, the budgets, and the repetitions; a result directory
  holds everything it produced.
- **attempt**: one stack building the product once inside a campaign. A
  campaign with three stacks and one repetition has three attempts.
- **execution**: one process run of an attempt. A retried attempt has two.
- **session**: one conversation with the coding agent. A build session writes
  the app; a repair session reacts to a failure report.
- **stack**: the technology under test, such as SpacetimeDB, PostgreSQL, or
  MongoDB. Flags still spell it `--backend`; the word is stack.
- **level**: one rung of a sequential campaign (L1, L2). **depth**: how far down
  the feature graph a dependency campaign has reached. They share a field but
  never a meaning.
- **feature**: one node of the dependency graph, the unit the agent builds and
  the grader scores. A feature opens when its parents pass.
- **questline**: a named path of features through the graph, such as identity
  or fulfilment. One questline can stop while the others continue.
- **check**: one scored criterion with a stable id such as `601b`. A **gate**
  check must pass before the feature's descendants open; a **guarantee** check
  costs points but never blocks.
- **disclosure**: whether a specification is requested (in the prompt),
  expected (not in the prompt, scored), or observed (not in the prompt, not
  scored).
- **first build**: the score before any repair. **repair**: one paid session
  that reacts to a failure report, plus the regrade after it. A repair candidate
  is accepted only under the mode's regression rules; rejected source remains
  available as evidence.
- **passed** and **failed** are measured outcomes; failed is the application's
  fault. **inconclusive** means the harness could not measure the check: no
  credit, no blame, and the reason is recorded. **blocked** means a prerequisite
  failed, so the check was not attempted.
- **harness failure** and **provider failure** mean the benchmark or the model
  provider broke; the attempt is **excluded** from comparison data, as is a
  **contaminated** attempt whose agent read grading material.
- **qualification**: matching reference, null-control, and defect-control
  evidence for an exact check selection. Without it, results are provisional.
- **needs attention**: a campaign that stopped and needs a person.
- **preflight**: the verifications before an attempt, each a **probe** such as
  `registry.cache`. A **smoke** preflight starts a real coding container without
  a model. **admission** is the record that preflight and policy allowed the
  campaign to start.
- **coding container**: the container the agent works in, created from the
  **build image**. It sees the app, its stack material, and nothing else.
- **clean source**: the accepted application source with nothing the agent's
  process left behind. Every grade starts the app from clean source.
- **credential broker**: the local proxy that holds the provider key so the
  coding container never sees it. Its **cost receipt** is the proof of what a
  session spent.
- **lease**: the record of which containers, ports, database, and locks an
  attempt owns, so cleanup and recovery act only on those.

## Data flow

```text
versioned definitions
        |
        v
compiled plan -> admission -> scheduler -> run engine -> grader
                                      |          |          |
                                      v          v          v
                                  state.json  run.json   evidence
                                      \          |          /
                                       \         v         /
                                        -> inspection -> report
```

The coding agent receives only the app request, current work, selected stack
material, and repair evidence allowed by the plan. It does not receive the
benchmark, grader, future work, expected implementation, or comparison data.

## Operator loop

An operator, human or agent, uses one loop:

1. **Define.** Select one versioned campaign file. Do not rebuild the plan from
   command flags.
2. **Validate.** Compile it and inspect the exact attempts, stacks, model,
   prompt policy, checks, points, budgets, images, and parallelism.
3. **Admit.** Prove credentials, images, ports, resource capacity, and stack
   access before model work starts.
4. **Run.** Start the exact stored plan or use an eligible continuation. A paid
   action is always explicit. Resume is not general process or database recovery.
5. **Observe.** Read durable campaign state first. Open logs only to diagnose a
   live phase or failure.
6. **Decide.** Continue only through a legal state transition. Never hide an
   invalid attempt or retry it outside the frozen policy.
7. **Report.** Generate the result from retained run evidence. Publish it as
   verified comparison data only when grading qualification is complete.
8. **Clean.** Remove temporary owned resources. Keep the campaign package.

The CLI and dashboard use the same compiler, scheduler, state reader, and run
commands. The dashboard is a view and input surface. It is not another control
plane.

## Agent interface

The operator interface must answer these questions without source inspection:

- What exact plan am I controlling?
- Can it start without spending model usage?
- What is running now, and in which phase?
- What has it cost and how long has it run?
- Which results are valid application results?
- Which failures belong to Stack Bench, the provider, the stack tools, the
  host, or the operator?
- What evidence proves each answer?
- Which actions are legal now?

Machine-facing commands return stable JSON. A compact response gives the plan
identity, campaign state, active work, cost, failures, and legal next actions.
Detailed responses add attempts and artifact paths. Logs and raw artifacts stay
available, but an operator does not need to parse them for normal control.

Errors must name the failed subsystem, failure owner, retryability, retained
evidence, and next safe action. `inconclusive` is an intermediate measurement
state, not an accepted final explanation.

## Resource rules

- Compile and inspect before any model call.
- Run focused source checks after a change. Run the integrated source gate once
  for the final source identity.
- Reuse qualification evidence only when its bound inputs match, or a validated
  evidence slice proves unchanged scope and a reviewed executable equivalence
  decision covers any runtime hash change. Preserve the original artifacts.
- Do not repeat reference, mutation, or null work for unchanged scope.
- Stop new paid attempts after a harness, provider, host, or operator failure.
- Retry only when the frozen attempt policy permits it. Extra repair grants
  require a separate operator action.
- Run independent attempts in parallel only within the plan and admitted host
  capacity.
- Preserve a failed package before a source or plan change.

## Accumulated knowledge

Operational knowledge belongs in typed artifacts, not chat transcripts or a
growing journal. Each completed action records its inputs, identity, outcome,
cost, duration, evidence paths, and owner. A later operator can reconstruct the
campaign from the retained package. Continuation still requires the engine's
eligibility checks; evidence alone cannot restore a lost live database or session.

Local notes can explain an active investigation. They cannot authorize a run,
change a score, or replace a missing artifact.

## Design test

Every major structure must have one purpose, one owner, and one current
consumer. If its reason cannot be stated in one sentence, simplify or remove it.
Complexity is allowed only when it protects result validity, isolation,
security, recovery, or a current operator need.
