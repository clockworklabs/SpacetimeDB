# Fairness

Clockwork Labs makes Spacetime, one of the stacks StackBench ranks. This page
records where the stacks are treated differently, and why.

## What every stack shares

- One feature list and one set of checks. Prompts differ only in each stack's
  setup and interface details.
- The same time limit and cost cap for every run in a study.
- Seeded bugs: for each stack, every L3 check must catch one before the checks
  count as qualified.
- The same network rules. Coding sessions cannot reach GitHub, where the checks
  are public. Grading cannot reach anything outside the attempt.

## Spacetime gets SDK skills

Spacetime agents also receive Spacetime's public agent skills for the TypeScript
server, TypeScript client, and CLI. They are installed in the agent's own skills
folder, and the prompt names the folder and the skills. Other stacks receive
their setup only.

- **Why:** Spacetime's current SDK is newer than the models' training data.
  Without the skills, agents write code for an older API.
- **What they are:** the skills a Spacetime user installs from the SpacetimeDB
  repository. Corrections found while building StackBench were checked against
  the CLI and SDK source and proposed as a change to those skills.
- **Comparing without them:** the `neutral-no-sdk` and `neutral-dev-no-sdk`
  profiles run Spacetime with no skills (see [Prompting](prompting.md#stack-setup)).

Spacetime agents also receive one short page StackBench wrote,
`backends/workflows/spacetime-dev.md`, on using `spacetime dev`, as part of the
prompt. The prompt check that refuses any hint of the hidden checks covers that
page. It skips only the public skills, which are product documents and discuss
testing.

## Which Spacetime runs

The controller builds the Spacetime CLI, server, and TypeScript SDK from the
repository commit it runs. Each run records that commit.

## Where stacks differ in grading

[Grading](grading.md) lists the differences that follow from how each stack
works:

- what a restart restarts
- how a crash interrupts checkout
- what counts as a refusal
- a failed Spacetime reset publish leaves the run unscored

When checks came out undecided only on Convex, the grader was fixed and the runs
repeated. Those runs were not scored as failures.
