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

### Outcome rules

- If an app prerequisite fails, the dependent checks are reported as **blocked**.
  They receive no credit, but this is not evidence that their target assertions
  failed. The prerequisite observation remains available for repair.
- Page navigation timeouts and other navigation transport failures are
  unmeasured: external resources can delay page readiness. Connection refusal is
  a measured reachability failure.
- Invalid selectors, grader scripts, and browser protocol errors are harness
  failures. Observation helpers must not convert these errors into missing controls.
- Harness and provider failures remain unmeasured and cannot become app failures.
- If the app itself stops answering mid-grading, checks measured before that keep
  their outcomes, the rest of the current work fails, and earlier features'
  unreached checks are recorded as not run. A readiness probe that times out is
  unmeasured, not an app failure.
- An application refusal is the stack's defined error result: HTTP 400, 401,
  403, 404, 409 or 422, a SpacetimeDB reducer failure (530), or a thrown
  `ConvexError`. SpacetimeDB's HTTP reply does not separate a deliberate
  reducer error from a panic, so a panicking reducer also counts as a refusal.
  An HTTP 500 does not.
- A request-tampering sign-in or sign-up step modifies the credential request the
  app actually sends. Positional arguments, such as a SpacetimeDB function call,
  take an added field only where the module schema names that parameter; a
  field with no parameter is recorded as absent and the request goes as sent. If
  the request cannot be captured or its parameters cannot be read, the step is
  unmeasured. An ordinary sign-in never stands in for the probe.
- Concurrent actions drain every branch before returning; measurement failures
  take priority over app failures. Check verdicts cannot contradict failed or
  unmeasured action evidence.
- Concurrent named calls retain every request outcome when cancelled, including
  responses received before cancellation. A lost response or request timeout is
  an unknown result, not proof that the app rejected or failed to commit the
  operation. Missing or unknown outcomes make the response assertion
  inconclusive, even if another request returned an app error. HTTP success
  alone does not prove the stored business effects.
- Bundle and dependency grading both inspect partial observations and cleanup
  evidence before accepting an app abort.
- Account checks require a real application session and an independently
  observed application-database write. Credential storage and log audits are
  source-specific diagnostics; they do not certify password storage or logging
  in arbitrary generated apps.

Campaign grading retries only the affected isolated suite, once, when its
evidence is explicitly retryable and inconclusive. It preserves completed suites
and both executions in the grade bundle and raw artifacts. Product failures,
mixed failure/inconclusive suites, cleanup failures and harness failures do not
retry. Qualification runs do not enable this policy. If grading remains
incomplete, it stops the attempt without treating the timeout as a failed feature
or selecting later work.

### Database reset between scenarios

Between isolated scenarios, every stack runs its normal application startup
after the database reset. This includes SpacetimeDB apps that perform
initialization outside module `init`. The harness does not guess migration names.
The shared agent request states that startup must initialize the supplied data
and accounts in an empty database, and preserve current quantities, prices, and
user data when a database already exists.

PostgreSQL resets recreate only the leased database, including its schema and
migration history. Build preparation, scenario isolation, and repair rollback
use the same reset. Durability probes restart services without resetting data.

Convex runs a pinned, self-hosted backend in each attempt's private network; no
cloud account is required. The grader uses the declared native operations and
independent database reads. Login probes preserve native WebSocket calls and
require matching replies. Authenticated replays use the observed native bearer
identity or session argument; missing or ambiguous credential transport remains
unmeasured. A Convex backend crash also stops its application functions, so the
grader measures that boundary once.

## Fault probes

The current campaign checks do not yet include controlled checkout write rejection
or forced scheduled-worker overlap. The rules below govern adding those probes;
they are not a claim of current coverage.

The purchasing and cart contracts do not fix order storage or ID generation.
An ID-collision probe verified on one saved app therefore cannot be applied to
all generated apps. Do not require sequential IDs just to make that probe work.
A general write-rejection probe needs an external fault method that supports the
app's actual storage, with proof that the intended write was rejected.

A database stall tests recovery from a stall. It does not by itself prove a late
write failed or that two workers selected the same job. Keep those claims distinct.

Scored fault probes leave the generated source and dependencies unchanged. Inject
faults through the isolated runtime or database, then check persisted application
state. Record the fault target, activation, release, and observed result. A setup
timeout or an unobserved fault is not an application failure or a pass.

Use instrumented copies only as grader controls, with their changes recorded.
Before promoting a probe, require normal-operation success, a known defect caught
at the intended check, a correct implementation passing under the same fault,
and successful recovery after release. An unsupported stack is not a passing
control; do not include the probe in a shared comparison until each stack has a
verified method for testing the same behavior.

A duplicate-checkout test does not establish rollback after a failed order write.
A restart test does not establish safety when scheduled workers overlap. Keep
those cases separate in check definitions and reported coverage.

Confirm the delay on at least one worker. Do not require a second worker to reach
the same write: correct job claiming can prevent it. Verify the final effect after
release, and check that a later poll does not repeat it.

## Failure reports

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

Live reference runs test that intended behavior passes. Null controls test that
an empty app fails each selected scored check conclusively. Live mutations test
that each selected check detects its assigned defect. These are finite controls
for an exact definition, not proof of general production readiness.

This command checks mutation definitions and source anchors only. It does not
start an app or show that the grader detects a defect:

```bash
npm run check:mutations -- --app <reference-app> --mutations <manifest>
```

For live controls, use the scoped commands in the
[reference guide](../reference-apps/README.md#live-qualification). Declare the
recipe and depth explicitly. A bare default command can measure a different scope.
During development, run only affected mutations. The full selected mutation set
is a release qualification gate and requires separate authorization.

The mutation runner requires:

- a fully passing clean baseline;
- one exact source anchor for every edit;
- a conclusive failure at the intended check;
- no unrelated failures;
- successful source restoration and app reset.

Setup, infrastructure, and inconclusive failures do not count as defect
detection. A surviving mutation can be equivalent, so confirm that its source
edit changes observable behavior before changing the check.

For concurrent checks, a defect control must preserve ordinary serial behavior.
For restart checks, ordinary execution must work before the restart. A disabled
operation does not isolate a race or a restart defect. Keep the baseline,
mutation source, action evidence, and cleanup outcome together. A control for one
defect does not validate all alternative implementations or failure modes.

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
