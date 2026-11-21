# Grader

The grader runs versioned scenarios against a generated store and records
browser, transport, lifecycle, and database evidence for each check. What the
outcomes mean and what the checks observe is in [grading](../docs/grading.md).

Each scenario actor gets its own browser context. A live-update check passes
only when the page that was already open changes. The grader never reloads a
failed assertion to try again.

## Database reset

Between check files, every stack runs the store's normal startup after the
database is emptied, including SpacetimeDB stores that initialize outside the
module's `init`. The prompt requires startup to create the starting data on an
empty database and to preserve existing data otherwise. PostgreSQL resets
recreate only the leased database. Durability probes restart services without
resetting data.

Convex runs a pinned self-hosted backend in each attempt's private network. A
Convex backend crash also stops its application functions, so the grader
measures that boundary once.

## Failure reports

An action never fails with a sentence. It fails with a finding from the closed
catalog in `src/actions/action-findings.ts`: a kind and its fields, where a
field is a control name, an action ID, an actor, a number, or an HTTP status.
Every reader renders the finding from its one template. Raw diagnostics travel
in a `detail` field that is never rendered.

## Scenarios

Scenario JSON holds actors, setup steps, actions, and scored checks. The
actions are compiled and registered in `src/actions/`; scenario prose is not
executable. Each stack adapter declares the capabilities it provides, and the
campaign compiler refuses a campaign that a selected stack could not measure.

When writing assertions:

- scope repeated elements to their owning row, order, or user;
- assert visible values, never the presence of an empty container;
- keep the original open page for live-update behavior;
- use separate actors for identity boundaries;
- say in the criterion's `note` why its points differ from its neighbors'.

```json
{ "do": "expect", "actor": "watcher", "testid": "item-stock",
  "in": { "testid": "item-card", "contains": "Air Purifier" }, "within": 10000 }
```

## Run it

`dist/commands/run-suite.js` is the normal entry: it owns database reset,
provenance checks, contract linting, scenario execution, logs, and the grade
bundle. Direct `dist/grader/grade.js` runs are for authoring one scenario:

```bash
node dist/grader/grade.js --url http://localhost:6173 --spec tracks/ecommerce/scenarios/01-account-create.json --label spacetime-l1 --out report.json
```

If the grader exits before writing JSON, read the retained
`grader-<suite>.stdout.log` and `grader-<suite>.stderr.log`.

`--media <dir>` records videos and failure screenshots; `--trace` adds a
Playwright trace, viewed with `npx playwright show-trace <trace.zip>`. Inspect
the failing actor's evidence before attributing a failure. Media is not tracked
in the repository.

Preflight binds the stack adapter, database or module name, ports, container
identity, and run lease, and the suite runner verifies that target before
grading. A mismatch is a harness failure.

Proving the checks against the reference apps, seeded bugs, and the empty app
is the [qualify-checks skill](../skills/qualify-checks/SKILL.md).
`npm run check:mutations -- --app <reference-app> --mutations <manifest>`
checks seeded-bug definitions and source anchors without starting a store.
