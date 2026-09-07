# Stack Bench grader

The grader runs versioned scenarios against a generated app. It collects
browser, transport, lifecycle, and database evidence for each check.

Each scenario actor receives a separate browser context. A live-update check
passes only when the page that was already open changes. The grader does not
reload a failed assertion and try again.

## Outcomes and scoring

Every check produces one outcome:

- `passed`;
- `failed`;
- `inconclusive` when required evidence is unavailable;
- `harness_failure` when Stack Bench could not perform the measurement.

Only a passed check adds its declared points. Other outcomes add zero and never
change the declared denominator. Console errors remain diagnostics and do not
change unrelated scores.

Authorization and replay checks pass only when the requested call ran and
produced verifiable evidence. Visible UI behavior cannot replace missing server
evidence.

An action never fails with a sentence. It fails with a finding from the closed
catalog in `src/actions/action-findings.ts`: a kind and its fields, where a
field is a contract control name, an action id, an actor label, a number, a
count, or an HTTP status. Every reader renders the finding from its one
template. Raw diagnostics travel in a `detail` field that is never rendered.

## Scenario ownership

Scenario JSON contains actors, setup steps, actions, and scored checks. The
action contracts are compiled and registered in `src/actions/`. Scenario prose
is not executable behavior.

Actions run through capability-scoped executors. Browser, transport,
concurrency, lifecycle, and database actions use the same typed result contract.
Each stack adapter declares the capabilities it provides and whether named
application actions travel as HTTP routes or reducer calls. The campaign
compiler resolves every selected check against every selected stack and
refuses a campaign that a stack could not measure.

When authoring assertions:

- scope repeated elements to their owning row, room, message, or user;
- assert visible values, not the presence of an empty container;
- require the original open page for live-update behavior;
- use separate actors for identity boundaries;
- say in the criterion's `note` why it carries its points when they differ
  from the feature's other criteria.

Example:

```json
{
  "do": "expect",
  "actor": "bob",
  "testid": "unread-badge",
  "in": { "testid": "room-item", "contains": "{room:unread-main}" },
  "within": 5000
}
```

## Run the grader

Use `dist/commands/run-suite.js` for normal grading. It owns database reset,
provenance checks, contract linting, scenario execution, logs, and bundle
creation.

Direct `dist/grader/grade.js` execution is for focused scenario authoring only:

```bash
node dist/grader/grade.js --url http://localhost:6173 \
  --spec tracks/ecommerce/scenarios/01-account-create.json \
  --label spacetime-l1 --out report.json
```

If the grader exits before writing JSON, inspect the retained
`grader-<suite>.stdout.log` and `grader-<suite>.stderr.log` files.

## Validate checks

Reference apps prove that intended behavior passes. Null controls prove that a
blank app cannot earn points. Mutations prove that each scored check detects its
assigned defect.

```bash
npm run test:null
npm run check:mutations -- --app <reference-app> --mutations <manifest>
```

During development, run only mutations affected by the change. Use the full
mutation set only when grader or check changes require it.

The mutation runner requires:

- a fully passing clean baseline;
- one exact source anchor for every edit;
- a conclusive failure at the intended check;
- no unrelated failures;
- successful source restoration and app reset.

Setup, infrastructure, and inconclusive failures do not count as defect
detection. A surviving mutation can be equivalent, so confirm that its source
edit changes observable behavior before changing the check.

## Media evidence

`--media <dir>` records videos and failure screenshots. `--trace` adds a
Playwright trace with DOM and network snapshots.

```bash
npx playwright show-trace <trace.zip>
```

Inspect the failing actor's evidence before attributing a failure. Media belongs
with run output and is not tracked in the repository.

## Execution target

Preflight binds the stack adapter, database or module name, ports, container
identity, and run lease. The suite runner verifies that exact target before
grading. A mismatch is a harness failure and cannot produce an application
score.

When several stacks fail the same check, inspect the structured evidence. A
shared failure is useful diagnostic information, but it does not prove whether
the apps or the check are wrong.
