# Author a benchmark change

Product text lives in `tracks/`, shared actions in `src/actions/`, and stack
operations in `src/stacks/`. A feature or rule change does not need runtime,
report, or dashboard edits. Each command below builds `dist` first.

## Add a feature

The customer profile is the worked example. Its complete path:

- `tracks/ecommerce/prompts/modular/customer-profile.md`: the product request
- `tracks/ecommerce/contracts/customer-profile.md`: the application interface
- `tracks/ecommerce/scenarios/progression-customer-profile.json`: the checks
- `tracks/ecommerce/composition/packs/progression-customer-profile.json`: the
  feature pack, its dependencies, and its selected checks
- `tracks/ecommerce/composition/recipes/progression-catalog.json`: the recipe
  that makes the pack available
- `tracks/ecommerce/progression/ecommerce.json`: the graph node and its parents

For a new feature, follow that path with new IDs. Write the request as a
customer would ("A customer can save and view a delivery note"). Put the
sample data in the scenario. Use existing actions
(`signUp`, `click`, `fill`, `expect`). Keep one criterion per behavior; do not
award several points for several selectors that prove the same thing. Add the
behavior to every reference app.

Validate, then review the rendered prompt for every stack and depth:

```sh
npm run pack -- validate tracks/ecommerce/composition/packs/<pack>.json --track ecommerce
npm run recipe -- validate tracks/ecommerce/composition/recipes/progression-catalog.json --track ecommerce
npm run recipe -- show tracks/ecommerce/composition/recipes/progression-catalog.json --track ecommerce
npm run check:scenarios
npm run check:composition
npm run check:prompts
```

## Add a production check

Put it in a `moduleType: "specification"` pack with a one-line product reason
(private delivery instructions belong to their owner), and select it through
the graph node's grading group. The feature request stays as it is. Do not add
probe strings or negative-test instructions to any stack's guidance.

Then add a seeded bug that breaks exactly that behavior while everything else
keeps working, declare the scenario and check ID it must fail, and run
`npm run test:mutation-definitions`. Static tests prove nothing about
detection; see [Qualification](#qualification).

## Match the observation to the claim

- **Session persistence**: reload, then observe the signed-in user with no
  sign-in step in between.
- **Server persistence**: observe from an independent client with no copied
  browser storage. A reload alone can pass from browser storage.
- **Restart survival**: prove the behavior works first, restart the owned
  runtime without reseeding, observe from a fresh client, and record which
  process restarted. A runtime-control failure is not an app defect.
- **Live updates**: set the observer's initial state, change data through
  another actor, then observe without a reload. The writer's own optimistic
  update is not evidence.
- **Autonomous execution**: closing clients is not enough if the next request
  runs the overdue work. Use an observation that cannot trigger it.
- **Absence**: establish readiness first. `waitUntilAbsent` tests eventual
  disappearance; `expect` with `absent: true` tests continued absence for its
  window. Neither proves permanent absence.
- **Contention**: prove a serial operation first, use independent actors,
  classify every request, and reconcile stored state after the burst. A seeded
  race bug must keep the serial operation working.
- **Timing**: prefer completion signals. Keep a fixed wait only when time is
  the behavior under test, and say why in the scenario.

Scenario steps can repeat with `{ "repeat": 10, "steps": [...] }` or
`{ "forEach": ["Beta", "Alpha"], "steps": [...] }`, where `"{value}"` takes
each listed string. Both compile to ordinary actions, at most 1,000 repetitions
and 10,000 steps.

When a valid store design reveals an assumption in a shared action, fix the shared action or
the scenario's entry sequence, and add the smallest executor test that shows
the failure.

## Change a requirement or a weight

Edit the request, the interface if needed, and the scenario assertions
together, keeping test input in the scenario. Keep a stable check ID only while
the criterion means the same thing; a materially different behavior gets a new
ID. Weights are the criterion's `points` in its scenario; nowhere else.

Save `recipe show` before and after, and compare selected checks, points, task
text, and hashes (`npm run recipe -- diff <old> <new> --track ecommerce`). Rerun
the scenario, composition, prompt, and seeded-bug checks, and `npm run graph`
if the graph changed.

## Add a study condition

Copy `conditions/guidance/neutral.json`, give it a new ID, register it in
`conditions/catalog.json`, and change only the treatment you want to compare.
Select it in a campaign definition. Do not add an agent-adapter branch for a
guidance change.

## Qualification

Any change to a prompt, scenario, reference app, weight, or the grading path
makes the existing evidence stale; produce new evidence with the
[qualify-checks skill](../skills/qualify-checks/SKILL.md). StackBench reads
qualification from evidence bound to the recipe hash. Do not add status fields
to recipes or references.
