# Author a benchmark change

Product text belongs in `tracks/`. Shared actions belong in `src/actions/`. Stack operations belong in `src/stacks/`. A normal feature or rule change does not need runtime, report, or dashboard edits.

Build once with `npm run build`. The commands below then use the compiled tools. Keep before and after outputs outside the authored definition directories. Do not copy old hashes into new evidence.

## Add a normal feature

Use the existing customer profile as a worked example. Its complete path is:

- `tracks/ecommerce/prompts/modular/customer-profile.md`: product request.
- `tracks/ecommerce/contracts/customer-profile.md`: stable application interface.
- `tracks/ecommerce/scenarios/progression-customer-profile.json`: observations and assertions.
- `tracks/ecommerce/composition/packs/progression-customer-profile.json`: feature, dependencies, and selected criteria.
- `tracks/ecommerce/composition/recipes/progression-catalog.json`: available packs.
- `tracks/ecommerce/progression/ecommerce.json`: graph ownership and dependencies.

For a new delivery-note feature, follow that path with new IDs. Ask for “A customer can save and view a delivery note.” Use a text field, save button, and summary hook. Use existing `signUp`, `click`, `fill`, and `expect` actions. Put the sample note in the scenario, not the product request. Keep one positive criterion for saving and viewing the note. Do not award several points for several selectors that prove the same behavior.

Create a feature pack with `moduleType: "feature"`. Declare its account dependency. Add the pack to the current recipe and give one graph node ownership of its grading group. Match the other graph nodes' `featureRefs`, `gradingGroups`, and `dependencies` format. Add the behavior to all reference stacks.

```sh
node dist/commands/composition-cli.js pack validate tracks/ecommerce/composition/packs/progression-customer-profile.json --track ecommerce
node dist/commands/composition-cli.js recipe validate tracks/ecommerce/composition/recipes/progression-catalog.json --track ecommerce
node dist/commands/composition-cli.js recipe show tracks/ecommerce/composition/recipes/progression-catalog.json --track ecommerce
node dist/commands/check-scenarios.js --track ecommerce --recipe progression-catalog.json
node dist/commands/check-composition.js
```

The example commands validate the existing profile path. Substitute the new pack path for the first command. The recipe output records selected check IDs, task fragments, and identities. Review the task text for every stack and selected depth. The dependency prompt contract test covers fresh builds and repair text through all current depths:

```sh
node --test dist/tests/dependency-neutral-prompt.contract.js
```

## Add an expected production check

The profile scenario also shows the negative case: another customer must not see the saved address. For the delivery note, save a note as one customer, open a separate account, and prove the note is absent. Use independent actors. Do not let a prior criterion's pass be the only evidence for setup.

Put the privacy criterion in a `moduleType: "specification"` pack with a product justification: private delivery instructions belong to their owner. Select it for scoring through the node's grading groups. The primary product request remains the normal feature request. A condition that explicitly supplies safeguards is a separate treatment. Do not add probe strings or negative-test instructions to general stack guidance. The intended SpacetimeDB skills remain enabled.

Prefer an authoritative fresh read after a write or replay. A blocked button alone does not prove server authorization. A request timeout, status 0, missing route, or server error does not prove correct rejection. For a successful authenticated replay, prove that the stored effect occurred once. Unauthorized replay must still be refused.

Add a mutant that exposes the other customer's note while keeping sign-up and saving functional. Declare the exact scenario and stable check ID. Run the existing anchor and syntax tests. Then obtain current baseline, null-control, and targeted mutation evidence before describing the new check as qualified. Static tests do not establish a mutation kill.

## Match the observation to the claim

- **Session persistence:** establish a session, reload, and observe the signed-in user without
  `signIn`, `signUp`, or `ensureSignedIn` between the reload and the observation.
- **Reload persistence:** save data, reload, and observe it. Signing in again can isolate data
  retention from session behavior. Browser storage survives reloads, so this alone does not
  establish server persistence.
- **Server persistence:** use an independent client without copied application storage, or
  suitable server evidence. Surviving a backend restart is a separate claim.
- **Restart survival:** first prove the saved state or ordinary scheduled operation works.
  Restart the owned runtime without reseeding its data, then use a fresh client to verify
  the result. Record which process restarted. A runtime-control failure is not an app
  defect. This does not establish power-loss, storage corruption, or database crash recovery.
- **Shared live updates:** establish the observer's initial state, change data through another
  actor, then observe without reload or re-navigation during the measured interval. Setup
  reloads are valid. The initiating client's optimistic update is not sufficient evidence.
- **Autonomous execution:** closing clients is insufficient if the next request runs overdue
  work. Use an observation that cannot trigger the work and a targeted negative control.
  Otherwise state the narrower behavior measured.
- **Absence:** establish readiness first. `waitUntilAbsent` tests eventual disappearance;
  `expect` with `absent: true` tests continued absence over its bounded `within` interval.
  Neither establishes permanent absence. Same-actor observations can validly test deletion
  or filtering; use an independent observer when the claim requires one.
- **Navigation:** reach the destination through disclosed controls. An optional control can
  be absent; the required destination cannot. Do not assume a toggle is idempotent or require
  a catalog round trip when the current view is already known.
- **Timing:** prefer completion signals. Keep elapsed-time waits when time is the behavior
  under test. Explain unavoidable fixed waits in the scenario. Budget the full execution
  path, including parallel branches, rather than only the longest individual wait.
- **Contention:** establish a successful serial operation, use independent actors, classify
  every request, and reconcile stored state after the burst. A timeout is an unknown
  business outcome until reconciled. Request overlap does not prove server execution
  overlap or sustained capacity. A race mutation must preserve the serial operation.
- **Disclosure:** review the actual compiled request at the relevant step, including retained
  contracts. Requested features may state timing or safeguards. Specification packs can
  measure expected production behavior without requesting it. Disclose necessary interface
  facts, but avoid layout restrictions and exact adversarial inputs.

When a valid interface reveals a driver assumption, fix the shared action or the scenario's
entry sequence. Extend the closest executor test with the smallest alternate interface that
demonstrates the failure. Assert the destination or result so a no-op cannot pass. These
fixtures validate driver behavior; they do not qualify an application's business behavior.

## Change a requirement or weight

For a changed delivery-note length limit, edit the feature request, interface only if needed, and scenario assertions together. Keep the test input in the scenario. Preserve a stable ID only while the criterion still means the same thing. Give a materially different behavior a new ID and remove the retired selection.

For a weight change, edit the criterion's `points` in its scenario. Check the compiled selection and graph score. Do not duplicate the weight in a report or UI. Check completion remains passed checks divided by the fixed selected check count; weighted score remains a separate measure. A zero-point control is excluded from scored check completion.

Save `recipe show` output before and after the edit. Compare the selected checks, point totals, task fragments, and meaning/execution/content hashes. Use `recipe diff <from> <to> --track ecommerce` when comparing two authored recipes already in the track. Do not keep temporary duplicate IDs in the pack catalog. Re-run the matching scenario, composition, prompt, and mutation-definition checks. Regenerate the graph with `npm run graph` if the graph changed.

## Add a study condition

Copy the shape of `conditions/guidance/neutral.json`, choose a new ID, and register it in `conditions/catalog.json`. Change only the treatment you intend to compare. Preserve application interface selection and recorded skill identities. Select the condition in a campaign definition and compile the campaign with the existing campaign command. Do not add an agent-adapter branch for a guidance change.

Declare repetitions, retry policy, budgets, and analysis before running. `analysis.spendThresholdsUsd` selects cost checkpoints; `analysis.completionTargets` selects target completion rates in `[0, 1]`. Reports use recorded grades only. Missing costs stay unknown and upper bounds remain marked. Cohorts retain stack, model, mode, level scope, skills, pricing, and definition identity, so a changed condition is not silently pooled with earlier results.

## Review and qualification

Check the rendered request and scoring scope separately. A prompt change, scenario change, reference fix, weight change, or runtime change makes evidence with the old identity stale. Current schemas derive qualification from identity-bound evidence; do not add retired `draft` or status fields to recipe or reference records. Pending qualification keeps scores provisional and blocks verified publication. It does not erase stored runs.

Before release, require a baseline pass, a nonfunctional control, and a targeted failing mutant for the intended verified scope. Record fresh-build results separately from post-feedback repairs. Report blocked and unmeasured checks as part of the full scope. See [the current coverage review](grading-coverage.md) for known gaps.
