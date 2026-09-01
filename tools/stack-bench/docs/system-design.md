# Stack Bench system design

Stack Bench turns one versioned test plan into verified comparison data. The
system must make every decision, action, result, and cost traceable without
using chat history or operator memory.

## One owner for each fact

| Layer | Owns | Durable output |
|---|---|---|
| Definitions | Product work, prompt modules, checks, stacks, models, and budgets | Versioned source files |
| Compiler | The exact work matrix and all bound identities | `plan.json` |
| Admission | Whether the exact plan can run on this appliance | Admission artifact |
| Scheduler | Attempt order, concurrency, continuations, and terminal state | `state.json` |
| Run engine | Build, grade, repair, resource ownership, and cleanup | Attempt directory |
| Grader | Typed check results and evidence | Grade artifacts |
| Progression engine | Open, passed, failed, and blocked features | `progression-state.json` |
| Report | A reproducible view of retained evidence | `report.json` and `report.html` |

No layer can silently replace a decision from a layer above it. A view can
summarize durable data, but it cannot create new run state.

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
3. **Admit.** Prove credentials, images, ports, resource capacity, stack access,
   and required qualification evidence before model work starts.
4. **Run.** Start or resume the exact stored plan. A paid action is always
   explicit.
5. **Observe.** Read durable campaign state first. Open logs only to diagnose a
   live phase or failure.
6. **Decide.** Continue only through a legal state transition. Never hide an
   invalid attempt or retry it automatically.
7. **Report.** Generate the comparison only from verified retained evidence.
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
- Reuse qualification evidence only when all bound hashes match.
- Do not repeat reference, mutation, or null work for unchanged scope.
- Stop new paid attempts after a harness, provider, host, or operator failure.
- Do not retry or grant more repair work automatically.
- Run independent attempts in parallel only within the plan and admitted host
  capacity.
- Preserve a failed package before a source or plan change.

## Accumulated knowledge

Operational knowledge belongs in typed artifacts, not chat transcripts or a
growing journal. Each completed action records its inputs, identity, outcome,
cost, duration, evidence paths, and owner. A later operator can reconstruct the
campaign from the retained package and continue from the last valid state.

Local notes can explain an active investigation. They cannot authorize a run,
change a score, or replace a missing artifact.

## Design test

Every major structure must have one purpose, one owner, and one current
consumer. If its reason cannot be stated in one sentence, simplify or remove it.
Complexity is allowed only when it protects result validity, isolation,
security, recovery, or a current operator need.
