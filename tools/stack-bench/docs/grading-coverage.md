# Grading coverage review

## Review failed checks

The shipping-result check waits for the declared submission state to report success, then
verifies fresh staff and customer views. The state stays on the fulfilment panel when the
shipped row disappears. It does not require the queue or customer view to update live. The
separate live fulfilment check covers new orders appearing in an open queue. It does not
establish live removal after shipping or live customer-status updates.

The separate shipping-accounting check uses the existing named shipping action with
staff credentials and the customer's declared order identifier. It waits for an accepted
server response before fresh order, stock, and revenue observations. This avoids a fixed
submission buffer in that production check; it does not test the shipping button. The
UI shipping check above still owns that interaction. Both changed scenarios are draft
pending matching live control evidence. The UI submission marker is app-reported evidence;
the fresh business observations still establish that shipping actually took effect.

Support refund accounting now checks a second, unrefunded order after replay and fresh
login. This detects refunds applied beyond the selected order. The changed check is draft
until matching live reference and defect evidence exists.

Return/refund checks at L6 exercise both operation orders. The product rule separates
stock receipt from money: accept the physical return once, restore stock once, and refund
only the amount still owed. Cumulative refunds cannot exceed the amount paid. The checks
read each warehouse, the refund total, and revenue. They remain draft pending matching live
controls. The new rule and checks do not apply retroactively to saved results.

The low-stock live check keeps its observer on the open list while a separate signed-in
administrator restocks. It does not assume that entering the admin area resets its subtab.

Application snapshots exclude `.log` and `.pid` files at every depth. This is a file-policy
boundary, not proof that each excluded file is disposable. Required source and seed inputs
must be retained in source files. Clean reconstruction must work without excluded runtime
files; source hashes alone cannot establish that. In-place restoration preserves runtime
logs, while a clean reset removes them. Use clean reconstruction for reproducibility claims.

A failed check records an observation that did not meet an assertion. It does not identify an
independent bug or prove its root cause. Several checks can fail from one missing
update path or one failed setup step.

For each investigated failure, keep a short review beside the retained evidence:

- Identify the campaign, execution, source hash, stable check IDs, and grade files.
- Record the observed result separately from the proposed cause. Read the action
  evidence and setup result before the final assertion. Link relevant traces,
  logs, and source lines.
- State whether evidence confirms an application defect or a harness defect, or
  whether the cause remains unresolved. Keep provider and interrupted outcomes
  separate. A valid failed assertion can have an unresolved application cause;
  an uncertain measurement cannot establish an application failure.
- Group checks only when evidence supports a shared cause. Keep every check's
  recorded outcome and score. Do not report the group count as a measured bug count.
- State what evidence is still needed and which focused check can supply it.

Keep claims within the measured boundary. A button shown to a guest proves a UI
visibility failure only when the contract forbids it. It does not prove the server
accepts a guest purchase. A missing stock number does not prove overselling. Use
direct-call results and stored quantities to assess those claims. Check whether
the supplied interface requires a number before blaming either app or grader for
an unreadable stock value.

If review confirms a grader defect, preserve the original artifacts and explain
which comparisons are invalid. The automatic report reads artifact outcomes; a
review note does not change its classifications or scores. Fix the shared grader,
verify the affected behavior, and regrade the unchanged saved app into separate
evidence. Record the corrected grader identity and its relationship to the original
result. Do not present the original affected score as a valid comparison or count
the regrade as a new independent app build. No new model generation is needed.

For dependency runs, a corrected gate can change which work the agent receives
next. A regrade can measure the saved application, but cannot reconstruct that
different development path. Keep it separate from new attempts under the corrected
definition. Show raw checkpoint checks beside accepted target completion and
blocked descendants; blocked checks are not independent observed failures.

### Replay a saved dependency candidate

Grading bundles include optional `phaseTimings` for application stop, database reset,
application start, readiness probes, and grader execution. Durations use a monotonic
clock and include failed operations. `suite: null` identifies preparation before
the scenario loop. `threw` records an exception, not whether a check passed; an
operation can return a failed result without throwing. The grader duration includes
its child process and evidence handling, so do not add it to the child grade duration.
The process log records final bundle writing and source verification separately.
These timings are diagnostics and do not change scores or timeout budgets.

Use the existing `run` command with `--grade-from`, an explicit `--grade-level`,
one or more `--check` IDs, and a fresh `--out` directory outside the original
execution. The depth selects that depth's saved first-build candidate. It does
not select the final accepted app, which can be an earlier depth after rejection.

```sh
node dist/commands/bench.js --grade-from /results/original/execution-1 \
  --grade-level 2 \
  --check ecommerce.spec.state-durability.session-reload.1e \
  --check ecommerce.spec.access-control.warehouse-write-boundary.103b \
  --out /results/diagnostics/session-and-authorization --no-media
```

Run this inside the configured Docker controller environment. The appliance
controller accepts the same arguments after `run`. No provider credentials or
model calls are required. The replay uses the original coding image, a fresh
owned backend and app directory, the saved credential aliases, and current checks.
It rebuilds the app from saved source; it does not restore old database contents.

Independent diagnostic commands can run in parallel. Each claims its app ports
and backend resources through the same lease system as campaigns. There is no
manually sized runner pool. Admission reports resource conflicts. Use a separate
output directory for each command. Replays
keep their saved run index and server endpoint, so candidates that need the same
ports must run at different times.

Also preserve the original generation dependencies. For example, regenerating
STDB bindings with a newer CLI can change their embedded version header and
correctly fail source verification. Set `STACK_BENCH_RELEASE_DEPS_VOLUME` to the
original release volume. Verify it with that release's immutable controller image
and `verify-deps`, mounted read-only. Then use Compose `run --no-deps` with the new
controller so its dependency initializer does not replace the old tooling. Keep
the new controller/backend identity in the diagnostic record; this is not a claim
that every runtime binary is identical to the original execution.

The source run must have finished and must not be contaminated. The selected
depth must identify one saved first-build candidate with matching source and
grading evidence. Checks must belong to that candidate's original scored scope.
Missing evidence, changed source, ambiguous depths, and overlapping output paths
are errors. An inconclusive original measurement can be investigated; the replay
does not make that original result valid.

Read `regrade.json` and its separate `grading/bundle.json`. The receipt identifies
the original run, source candidate, original grading evidence, current definition,
and cleanup result. It is diagnostic evidence, not a campaign run. Do not add its
checks or zero additional model cost as another trial in a stack comparison.

Without `--grade-level`, the existing single-level sequential regrade retains its
original product-contract and scoring-scope checks. Dependency replay deliberately
permits current check definitions and records the difference.

Scenario navigation must work with both separate pages and single-page views.
After reload, reopen a declared entry control when it exists, then require the
destination to be visible before inspecting its contents. An absence assertion
must not pass merely because the whole view is closed. Select account rows by
account identity, not text shared by their role options. Privacy checks must use
a refreshed positive control when live propagation has its own check.

## Stock alert observation boundary

The notification destination marker identifies the opened view, including while it loads.
Its aria-busy attribute is false only after the signed-in account's list loads successfully,
including an empty result. Loading or failed reads cannot earn empty-list credit. Entering
notifications must preserve an already-open destination; ordinary toggles may close it.
The initial alert request must report successful submission on its item card before the
first restock. Rejected or unconfirmed submission stops setup instead of becoming a missing
delivery failure. This replaces the fixed 2.5-second buffer. The marker is app-reported;
it does not independently prove a saved subscription. The later delivery check still verifies
the business effect.

The duplicate-alert check samples a fresh client's loaded list after a ten-second wait following
the second restock. It checks one persisted alert at that observation point. It is not continuous
observation and does not exclude duplicates created later. The fresh client avoids relying on
an unchanged list in the initiating browser. A read that itself triggers overdue work can still
pass; this does not establish autonomous notification execution. Negative controls and live
reference evidence must match the changed scenario, interface, and reference identities.
The destination/readiness and submission-state markers are new interface requirements.
Preserve old runs under their original definition; do not count missing markers in a saved
application as agent failures. Changed marker scenarios remain draft until live references
and relevant defect controls match the new definition.

## Qualification and source coverage

### Stored state and contention

Stock observations use the same item, warehouse, and stock interface already
required for external corrections. Reads use the authenticated backend lease.
Zero and negative quantities remain observations. Missing, invalid, or ambiguous
data cannot become a fabricated zero or a passing comparison. PostgreSQL resolves
the declared relational links; MongoDB and SpacetimeDB use their declared stock
interfaces. This does not provide an independent read of arbitrary application
tables, such as payments or orders.

The restock race requires an ordinary stored restock in setup. Its scored check
then verifies stored stock, each buyer's order, and UI agreement. A failed setup
does not establish a concurrency defect. A stock assertion does not establish
that every other stored entity is correct.

Named concurrent calls retain request timing and distinguish responses, transport
errors, and timeouts. A timeout has no known business outcome until state is
reconciled. Client request overlap is not proof of overlap inside the server.
The optional [contention diagnostic](development.md#optional-contention-diagnostic)
uses repeated request bursts. It is not a sustained capacity test or a change to
the scored campaign target.

These changes are draft. Compilation, synthetic transport tests, and isolated
stock-reader checks do not replace live reference and targeted-defect evidence.
For a race control, preserve ordinary serial behavior and challenge the concurrent
case. For restart survival, preserve execution before restart. A disabled timer
only proves detection of absent execution, not restart-specific loss. The restart
probe first completes an identical ordinary timer. PostgreSQL and MongoDB controls
remove pending work at startup. The SpacetimeDB control keeps pending rows but loses
its process-local execution queue; isolate replacement can also lose that queue.

The restock probe first verifies an ordinary purchase and restock. PostgreSQL and
MongoDB controls replace atomic reservation with an unlocked read and absolute
write. Fixed delays widen overlap in defect controls only; they do not measure a
natural failure rate. SpacetimeDB reducers remain atomic. Its control sends stale
absolute stock from the client, then overwrites intervening purchases. These are
distinct ways to break the same stock invariant, not equivalent internal races.

Source coverage and executed controls are separate evidence. A declared mutation target is
not a successful control, and a failed setup is not a target kill. Historical inventory
counts do not establish the state of a changed definition.

Use the current graph, recipe, reference registry, and mutation manifests as the source of
truth. The [mutation coverage checks](../tests/progression.mutation.ts) report missing exact
depth 1–3 targets. Source checks do not replace live controls.

Current qualification is pending. Before a verified comparison, resolve material defects in
the selected scope and collect matching reference, null-control, and mutation evidence.
Keep missing definitions, unexecuted controls, failed or surviving controls, and stale evidence
separate. Record the exact source, engine, recipe, fixture, and result identities beside each
executed control. Do not copy old evidence into a changed calibration.

An exploratory campaign can proceed with pending qualification, but its scores are
provisional. A gap outside its selected scope does not block it. A signed distribution and
its [release verification](../appliance/RELEASE.md) are separate from grading qualification.

For each content finding, record the check and owner, material delivered at the relevant
step, observation and timing assumptions, a valid alternative implementation, control
evidence, and disposition. Review changed content and unresolved findings; do not repeat
an audit of unchanged material. Follow [authoring rules](authoring.md) when extending the
workload. Source inspection alone cannot qualify new depths or alternative interfaces.

## Expected production criteria

The current specification families have a product reason. They do not need a general request to “build production software.” Their scope must still follow the selected product features. These are semantic review findings, not a claim that live grading is qualified.

| Specification family | Product reason and acceptance rule |
| --- | --- |
| `ecommerce.spec.state-durability` | Separate session continuity from saved data. Check cart, orders, profile, preferences, staff roles, and support history after runtime restart and fresh login. Retain reload checks for browser continuity. |
| `ecommerce.spec.access-control` | Customer data and staff operations have different owners. Test direct server calls as well as the visible interface. A refusal must have a defined result; transport failure is not proof. |
| `ecommerce.spec.live-state` | Shared catalog, inventory, cart, and operations views must reflect changes where the product calls for live information. Use distinct actors and bounded waits. Polling is acceptable when it meets the same observable rule. |
| `ecommerce.spec.concurrency-safety` | Several valid customers can act at once. Classify every request, allow stack-specific conflict results, and prove stock/cart/order invariants. Do not prescribe locks, reducers, or queue design. |
| `ecommerce.spec.external-data-sync` | A shared data view must not depend only on one client's local writes. Retain equivalent stack-specific mutation paths and fresh observations. |
| `ecommerce.spec.transactional-integrity` | Stock and money cannot be created or lost by partial operations. Prove the before/after quantities and totals, including rejected overdrafts. |
| `ecommerce.progression.cancellation-queue-specifications` | A cancelled order must leave the work queue. Keep queue visibility distinct from monetary accounting. |
| `ecommerce.progression.cancellation-accounting-specifications` | Cancellation must reverse only the appropriate stock and revenue effects. Repeat requests cannot reverse them twice. |
| `ecommerce.progression.price-accounting-specifications` | Current price edits must not rewrite earned revenue. Historical and future prices have different meanings. |
| `ecommerce.progression.price-history-specifications` | A buyer's receipt must keep the agreed purchase price. The scenario owns exact probe prices. |
| `ecommerce.progression.inventory-conservation-specifications` | A warehouse transfer changes location, not total inventory. Reject insufficient stock with no partial effect. |
| `ecommerce.progression.operations-access-specifications` | Administrative changes must follow product roles. A hidden button alone is insufficient. |
| `ecommerce.l3.deferred-access-specifications` | Scheduled work is still an authorized business operation. Schedule creation and execution cannot bypass access rules. |
| `ecommerce.l3.deferred-durability-specifications` | Reservations and scheduled restocks must survive the specified restart. Do not score a restart failure as an application assertion. |
| `ecommerce.l3.deferred-integrity-specifications` | Deferred work must produce one business effect. A replay can return success when stored state still proves one effect. |
| `ecommerce.l3.server-time-specifications` | Observe reservation expiry with its browser closed and scheduled work after restart. These probes do not change the host or client clock. They do not establish clock-skew tolerance. |
| `ecommerce.progression.review-access-specifications` | Review ownership and visibility follow the product's role rules. Exercise the direct access path and an independent observer. |

Keep these rules. Keep the workload breadth and intended SpacetimeDB skills. Keep first-build measurements separate from post-feedback repairs. Do not call a score “production readiness”: these checks cover the declared product behaviors, not all security, accessibility, operational, or performance requirements of a deployed service.

Concurrent requests need classified outcomes. Keep adversarial values in scenarios, not
product interfaces. For authenticated idempotent replay, verify unchanged totals and one
business record through a fresh authoritative read. Unauthorized replay remains a separate
refusal check.

Review limits remain: static source inspection cannot prove timing thresholds are attainable on the Docker appliance, that all reference stacks pass, or that a mutant fails only its target. Those are release qualification gates. Current qualification remains pending. No weight changes or broad feature removals are justified by the present evidence alone.

## September 2026 probe audit

The source review covers all four ecommerce recipes, the full dependency graph through
depth five, both chat levels, and the separate contention diagnostics. Resolve depth from
the graph; a scenario's level field is not its dependency depth. Review selected criteria,
including their retained setup, rather than counting every criterion in a source file.

The corrections reuse existing actors, actions, stock reads, and lifecycle controls:

- Privacy observes scoped private data in responses as well as the page. Capture includes
  HTML, JSON, text, native EventSource, and the existing WebSocket decoder. Positive owner
  observations establish that the data was delivered. Dropped, unreadable, or unfinished
  evidence cannot establish absence. Positive observations can wait for pending responses.
- Price, transfer, cancellation, and return checks prove the original business effect before
  its preservation or reversal. Transfer races check each stored warehouse quantity as well
  as the total. Authorized operations establish a working route before refusal checks.
- Cross-owner order checks combine the attacker's case with the other owner's order.
  DOM parameter overrides preserve the caller's credentials and the adapter's wire types.
- Reorder, recommendation dismissal, notification privacy, and resync checks retain the
  prerequisites needed when only that criterion is selected. Idempotent success is accepted
  when fresh observations prove one business effect.
- Chat disappearance checks wait for the transition. Confidentiality checks retain a full
  absence window. Room creation and room entry are separate actions.

These changes follow the distinction between an interface check and a server authorization
check in the [OWASP authorization testing guide](https://owasp.org/www-project-web-security-testing-guide/v42/4-Web_Application_Security_Testing/05-Authorization_Testing/02-Testing_for_Bypassing_Authorization_Schema),
and its advice to verify business data in [integrity tests](https://owasp.org/www-project-web-security-testing-guide/v42/4-Web_Application_Security_Testing/10-Business_Logic_Testing/03-Test_Integrity_Checks).

The supported claims remain finite:

| Observation | Does not establish |
| --- | --- |
| Hosted app restart for PostgreSQL/MongoDB; SpacetimeDB runtime restart with retained data | Common database crash semantics, power-loss recovery, or corruption recovery |
| Private marker absent from supported captured responses | All endpoints, encodings, binary formats, or arbitrary object-reference attacks |
| Exact final stock, orders, and totals | Every intermediate state, general serializability, or an external payment ledger |
| Bounded concurrent requests | Sustained throughput, many independent users, or server execution overlap |
| Serial promotion redemption limit | Concurrent competition for the last redemption |
| Hidden return/activity controls and displayed activity fields | Server-side return authorization, audit-log confidentiality, or tamper evidence |

Changed calibration inputs remain drafts with no imported qualification evidence. Runtime
budgets are planning ceilings; changed restart/observation allowances need fresh measurements.
Matching live references and defect controls are still required before a verified comparison.
Chat has additional qualification blockers recorded in [its level notes](../tracks/chat/LEVELS.md).
Passing source checks or an exploratory paid cohort does not remove these limits.


### Staff-role authorization follow-up

Criterion 621b now requests a different role in both HTTP replay and reducer replay,
then reloads the administrator view to verify that a rejected request changed no role.
PostgreSQL and MongoDB have a denied-after-write defect control. SpacetimeDB reducer
rejection rolls back the transaction; its existing unauthorized-acceptance control
remains applicable. These changed checks have no matching live qualification evidence
and remain draft.

Criterion 621d now checks administrator-role removal using the same signed-in staff
session before and after removal. The product policy grants administrator access to
`admin`, and staff access without administrator access to `staff` and `inventory`.
Reference role assignment updates the existing persisted administrator flag. The
probe proves a successful role change while authorized, then verifies denial and
unchanged stored role after administrator access is removed. Each stack has a
control that retains administrator access after removal. Matching live qualification
is pending; this check does not establish subscription revocation or token logout.
