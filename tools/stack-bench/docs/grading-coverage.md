# Grading coverage and limits

This guide explains what the checks observe, what they do not establish, and how
to review a failure. Current check counts and points come from the
[compiled recipe](../tracks/ecommerce/composition/README.md); qualification
status comes from the calibration and its evidence
(`qualification status --track <track> --level <N> --recipe <recipe>`). This
document does not maintain a second count or status.

Old runs keep their original definition; a new interface requirement is not counted against a saved application that
predates it.

## Scored checks and diagnostics

Scored checks belong to the selected recipe and count toward completion.
Diagnostics are reference-only observations outside scored campaigns. They add no
feature points and do not change historical results.

- **Purchase contention** (optional purchase, scarce-stock, and restock
  scenarios) uses the request recorder and verified reference database readers.
  It compares accepted purchases with each buyer's new orders and payments,
  preserves earlier records, and reconciles stock against order allocations and
  restock requests. Scarce stock limits accepted sales; ample stock requires every
  purchase to succeed. It establishes net per-warehouse conservation and
  per-buyer counts. It does not identify each order by a durable request ID,
  expose every compensating error, or prove intermediate state, server execution
  overlap, crash safety, or sustained throughput.
- **Concurrent cancellation** (`diagnostic-cancellation-contention.json`) sends
  overlapping cancellation calls from two sessions of one account. It verifies
  the pending order first, records every request outcome, then checks stored
  warehouse allocations, cancelled status, preserved order and payment history,
  and fresh revenue. Repeated calls may refuse or succeed without additional
  effects. It covers non-credit orders only.

Both use verified reference schemas. Saved model apps need verified reader
mappings before these observations apply. The
[contention diagnostic](development.md#optional-contention-diagnostic) runs
repeated request bursts; it is not a capacity test.

## Observation rules

- **Navigation.** Scenarios work with separate pages and single-page views.
  After reload, reopen a declared entry control when it exists, then require the
  destination to be visible before inspecting it. An absence assertion must not
  pass merely because the whole view is closed. Select account rows by account
  identity, not text shared by their role options.
- **Readiness markers.** A destination marker identifies the opened view,
  including while it loads. `aria-busy` is false only after the signed-in
  account's list loads successfully, including an empty result, so loading or
  failed reads cannot earn empty-list credit. Submission-state markers report
  success on the acting control before the grader proceeds. Markers are
  app-reported evidence; fresh business observations still establish the effect.
- **Stored stock.** Stock reads use the item, warehouse, and stock interface
  already required for external corrections, through the authenticated backend
  lease. Zero and negative quantities remain observations. Missing, invalid, or
  ambiguous data cannot become a fabricated zero or a passing comparison.
  PostgreSQL resolves the declared relational links; MongoDB and SpacetimeDB use
  their declared stock interfaces. This is not an independent read of arbitrary
  application tables such as payments or orders.
- **Business effects.** Price, transfer, cancellation, and return checks prove the
  original business effect before its preservation or reversal. Authorized
  operations establish a working route before refusal checks. Idempotent success
  is accepted when fresh observations prove one business effect.
- **Live updates.** Dedicated live-update checks keep their observers on the open
  page. Other checks use fresh views, and stored reads follow the live assertions
  so they cannot give the UI extra time.
- **Concurrent calls.** Named concurrent calls retain request timing and
  distinguish responses, transport errors, and timeouts. A timeout has no known
  business outcome until state is reconciled. Client request overlap is not proof
  of overlap inside the server.
- **Privacy.** Capture includes HTML, JSON, text, native EventSource, and the
  WebSocket decoder. Positive owner observations establish that the data was
  delivered. Dropped, unreadable, or unfinished evidence cannot establish absence.
  Fetch-based SSE streams are not supported and fail closed.
  Actors with a selected `expectNotReceived` observation disable browser HTTP
  caching before navigation, including fresh and reopened clients. Chromium can
  discard decoded font bodies and cannot expose those bytes on later cache hits.
  These privacy checks measure fresh HTTP responses; they do not establish HTTP
  cache isolation. Other actors retain normal caching. No domain or asset type
  is exempted from the existing capture rules.
  At the end of the observation window, pending body reads get at most one second
  to finish. Reads still pending after that limit remain inconclusive.
- **Deferred work.** Checks anchor on the original time and use early and late
  observations. Missing an observation window is unmeasured, not an app failure.
  The probes do not change the host or client clock.
- **Source snapshots.** Application snapshots exclude `.log` and `.pid` files at
  every depth. Required source and seed inputs must be in source files. Clean
  reconstruction must work without excluded runtime files; use clean
  reconstruction for reproducibility claims.

## Notes on specific checks

- **Shipping result** waits for the declared submission state, then verifies
  fresh staff and customer views. It does not require the queue or customer view
  to update live. The separate live fulfilment check covers new orders appearing
  in an open queue, not live removal or live customer-status updates.
- **Shipping accounting** uses the named shipping action with staff credentials
  and waits for an accepted server response before fresh order, stock, and
  revenue reads. It does not test the shipping button; the UI check owns that.
- **Support refund accounting** checks a second, unrefunded order after replay and
  fresh login, detecting refunds applied beyond the selected order.
- **Return and refund (L6)** exercises both operation orders. Accept the physical
  return once, restore stock once, and refund only the amount still owed.
  Cumulative refunds cannot exceed the amount paid.
- **Low-stock live** keeps its observer on the open list while a separate
  administrator restocks. It does not assume the admin area resets its subtab.
- **Stock alerts.** The initial alert request must report successful submission
  before the first restock; a rejected or unconfirmed submission stops setup. The
  duplicate-alert check samples a fresh client ten seconds after the second
  restock. It is not continuous observation and does not exclude later
  duplicates. A read that triggers overdue work can still pass, so this does not
  establish autonomous notification execution.
- **Restock race** requires an ordinary stored restock in setup, then verifies
  stored stock, each buyer's order, and UI agreement.
- **Staff roles (621b)** requests a different role through HTTP and reducer
  replay, then reloads the administrator view to verify no role changed. **621d**
  checks administrator-role removal with the same signed-in staff session before
  and after. `admin` grants administrator access; `staff` and `inventory` do not.
  It does not establish subscription revocation or token logout.
- **Login input (101a)** replaces the password in one captured JSON login request
  with query-like text. A protected purchase must be refused and stored stock
  unchanged, with normal login working before and after. This does not establish
  general SQL or NoSQL injection safety. Missing, ambiguous, repeated, redirected,
  non-JSON, or incomplete captures are inconclusive.
- **Checkout crash integrity and acknowledged-order durability** are owned by the
  checkout feature. PostgreSQL and MongoDB use separate application and database
  process crashes. SpacetimeDB uses one combined process crash, with an explicit
  shared observation for the application boundary. An uncertain outcome or a
  missed fault window is unmeasured.

## Expected production criteria

Each specification family has a product reason. Its scope follows the selected
product features.

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
| `ecommerce.l3.server-time-specifications` | Observe reservation expiry with its browser closed and scheduled work after restart. These probes do not establish clock-skew tolerance. |
| `ecommerce.progression.review-access-specifications` | Review ownership and visibility follow the product's role rules. Exercise the direct access path and an independent observer. |

Keep first-build measurements separate from post-feedback repairs. Do not call a
score “production readiness”: these checks cover the declared product behaviors,
not all security, accessibility, operational, or performance requirements of a
deployed service.

Concurrent requests need classified outcomes. Keep adversarial values in
scenarios, not product interfaces. For authenticated idempotent replay, verify
unchanged totals and one business record through a fresh authoritative read.
Unauthorized replay remains a separate refusal check. These rules follow the
distinction between interface and server authorization checks in the
[OWASP authorization testing guide](https://owasp.org/www-project-web-security-testing-guide/v42/4-Web_Application_Security_Testing/05-Authorization_Testing/02-Testing_for_Bypassing_Authorization_Schema),
and its advice to verify business data in
[integrity tests](https://owasp.org/www-project-web-security-testing-guide/v42/4-Web_Application_Security_Testing/10-Business_Logic_Testing/03-Test_Integrity_Checks).

## Claim limits

| Observation | Does not establish |
| --- | --- |
| Hosted app restart for PostgreSQL/MongoDB; SpacetimeDB runtime restart with retained data | Common database crash semantics, power-loss recovery, or corruption recovery |
| Checkout interrupted by an application or database process kill; recovered cart and orders reconciled with recorded requests | Power loss, disk corruption, every crash timing, or external payment durability |
| Private marker absent from supported captured responses | All endpoints, encodings, binary formats, or arbitrary object-reference attacks |
| Exact final stock, orders, and totals | Every intermediate state, general serializability, or an external payment ledger |
| Bounded concurrent requests | Sustained throughput, many independent users, or server execution overlap |
| Serial promotion redemption limit | Concurrent competition for the last redemption |
| Hidden return/activity controls and displayed activity fields | Server-side return authorization, audit-log confidentiality, or tamper evidence |

Later-depth limits are listed with the [ecommerce levels](../tracks/ecommerce/LEVELS.md#later-depths-l4l6).
Chat has additional qualification blockers in [its level notes](../tracks/chat/LEVELS.md).

## Controls and qualification

Source coverage and executed controls are separate evidence. A declared mutation
target is not a successful control, and a failed setup is not a target kill.
Use the current graph, recipe, reference registry, and mutation manifests as the
source of truth. The [mutation coverage checks](../tests/progression.mutation.ts)
report missing exact depth 1–3 targets. Source checks do not replace live controls.

For a race control, preserve ordinary serial behavior and challenge the
concurrent case. For restart survival, preserve execution before restart; a
disabled timer only proves detection of absent execution. The restart probe first
completes an identical ordinary timer. PostgreSQL and MongoDB controls remove
pending work at startup. The SpacetimeDB control keeps pending rows but loses its
process-local execution queue.

The restock probe first verifies an ordinary purchase and restock. PostgreSQL and
MongoDB controls replace atomic reservation with an unlocked read and absolute
write. SpacetimeDB reducers remain atomic; its control sends stale absolute stock
from the client, then overwrites intervening purchases. These are distinct ways
to break the same stock invariant. Fixed delays widen overlap in defect controls
only; they do not measure a natural failure rate.

Before a verified comparison, resolve material defects in the selected scope and
collect matching reference, null-control, and mutation evidence. Keep missing
definitions, unexecuted controls, failed or surviving controls, and stale evidence
separate. Record the exact source, engine, recipe, fixture, and result identities
beside each executed control. Do not copy old evidence into a changed calibration.
Static source inspection cannot prove that timing thresholds are attainable on the
appliance, that all reference stacks pass, or that a mutant fails only its target.

An exploratory campaign can proceed with pending qualification, but its scores
are provisional. A gap outside its selected scope does not block it. A signed
distribution and its [release verification](../appliance/RELEASE.md) are separate
from grading qualification.

For each content finding, record the check and owner, material delivered at the
relevant step, observation and timing assumptions, a valid alternative
implementation, control evidence, and disposition. Follow the
[authoring rules](authoring.md) when extending the workload.

## Review a failed check

A failed check records an observation that did not meet an assertion. It does not
identify an independent bug or prove its root cause. Several checks can fail from
one missing update path or one failed setup step.

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
direct-call results and stored quantities to assess those claims.

If review confirms a grader defect, preserve the original artifacts and explain
which comparisons are invalid. The automatic report reads artifact outcomes; a
review note does not change its classifications or scores. Fix the shared grader,
verify the affected behavior, and regrade the unchanged saved app into separate
evidence. Record the corrected grader identity and its relationship to the original
result. Do not present the original affected score as a valid comparison or count
the regrade as a new independent app build.

For dependency runs, a corrected gate can change which work the agent receives
next. A regrade can measure the saved application, but cannot reconstruct that
different development path. Keep it separate from new attempts under the corrected
definition. Show raw checkpoint checks beside accepted target completion and
blocked descendants; blocked checks are not independent observed failures.

### Replay a saved dependency candidate

Use the `run` command with `--grade-from`, an explicit `--grade-level`, one or
more `--check` IDs, and a fresh `--out` directory outside the original execution.
The depth selects that depth's saved first-build candidate. It does not select
the final accepted app, which can be an earlier depth after rejection.

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
and backend resources through the same lease system as campaigns, and admission
reports resource conflicts. Use a separate output directory for each command.
Replays keep their saved run index and server endpoint, so candidates that need
the same ports must run at different times.

Preserve the original generation dependencies. For example, regenerating STDB
bindings with a newer CLI can change their embedded version header and correctly
fail source verification. Set `STACK_BENCH_RELEASE_DEPS_VOLUME` to the original
release volume. Verify it with that release's immutable controller image and
`verify-deps`, mounted read-only. Then use Compose `run --no-deps` with the new
controller so its dependency initializer does not replace the old tooling. Keep
the new controller and backend identity in the diagnostic record.

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

Without `--grade-level`, the single-level sequential regrade retains its original
product-contract and scoring-scope checks. Dependency replay deliberately permits
current check definitions and records the difference.

Grading bundles include optional `phaseTimings` for application stop, database
reset, application start, readiness probes, and grader execution. Durations use a
monotonic clock and include failed operations. `suite: null` identifies
preparation before the scenario loop. `threw` records an exception, not whether a
check passed. The grader duration includes its child process and evidence
handling, so do not add it to the child grade duration. These timings are
diagnostics and do not change scores or timeout budgets.
