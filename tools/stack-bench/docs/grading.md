# Grading

This is how StackBench scores a store: what each check outcome means, how
outcomes become a score, what the checks observe, and what they do not prove.
Check counts and points come from the compiled recipe, and qualification status
from `qualification status`; this document holds neither.

## Outcomes

Every check ends in one outcome:

- `passed`: every assertion held.
- `failed`: an assertion did not hold. This is a conclusive application failure.
- `blocked`: a prerequisite of the check failed, so the check never ran. This
  is not evidence about the check's own assertion.
- `inconclusive`: StackBench could not reach a verdict, for example a response
  that never finished or a fault that missed its window.
- `harness_failure`: StackBench itself failed, such as an invalid selector or a
  browser protocol error.

A result is **conclusive** when it is `passed` or `failed`. Only `passed` adds
the check's points. The other outcomes add nothing and never change the
denominator. Harness and provider failures never become application failures.

Rules that decide the outcome:

- A refusal is the stack's defined error result: HTTP 400, 401, 403, 409, or
  422; HTTP 404 only after the same operation succeeded for an authorized
  caller; a SpacetimeDB reducer failure (HTTP 530, which also covers a panic);
  or a thrown `ConvexError`. HTTP 500 is never a refusal. A missing route or an
  incomplete response cannot prove authorization, so a refusal check without a
  proven route is inconclusive.
- A page navigation timeout is inconclusive, because external resources can
  delay readiness. A refused connection is a measured failure.
- If the store stops answering mid-grading, checks already measured keep their
  outcomes, the rest of the current work fails, and unreached checks are
  recorded as not run.
- Concurrent calls keep every request's outcome. A lost response or a timeout
  is unknown; it is not proof of a rejection or a commit, and makes the response
  assertion inconclusive. HTTP success alone does not prove the stored effect.
- A check file whose inconclusive results are all retryable runs once more on a
  fresh database. Failures, cleanup failures, and harness failures do not retry.

## Categories and counting

Each check has a category, which only splits reports:

- **Feature**: a requested product operation or result.
- **Production**: an access boundary, ownership rule, durability, concurrency,
  deduplication, or synchronization property.
- **Interface**: presentation or a test hook with no separate assertion.
- **Unknown**: the frozen definition predates categories.

Three measures share one denominator, the frozen selection, including blocked
and unmeasured work:

- **Check completion**: passed checks over selected point-scoring checks.
- **Feature completion**: fully passed features over selected features. A
  feature with a failed production check is not complete.
- **Weighted score**: passed points over selected points.

Score on the leaderboard is feature completion averaged over valid runs. Checks
that score zero points, such as setup controls, are evidence only.

## What the checks observe

- **Pages.** Scenarios work with separate pages and single-page views. After a
  reload they reopen the declared entry control and require the destination to
  be visible before reading it. An absence assertion never passes because the
  whole view is closed.
- **Readiness markers.** A destination marker identifies the opened view. An
  `aria-busy="false"` marker means the signed-in account's list loaded, so a
  failed read cannot earn empty-list credit. Markers are app-reported; the
  business effect is still read fresh.
- **Stored data.** StackBench reads only the stock, order, and cart tables the
  prompt requires, through the leased database. Missing or ambiguous data is
  inconclusive, never a fabricated zero.
- **Business effects.** Price, transfer, cancellation, and return checks prove
  the original effect before testing its preservation or reversal. An authorized
  call proves the route before a refusal check uses it.
- **Live updates.** Live-update checks keep the observer on the page it already
  had open. Every other check reads a fresh view.
- **Concurrent calls.** Timing is recorded on the client. Overlap on the way to
  the server is not proof of overlap inside it.
- **Privacy.** StackBench records every HTTP response and WebSocket message a
  browser receives, with the cache off, and searches for the owner's marker.
  The owner's own recording must contain it. Unreadable or unfinished traffic
  makes the check inconclusive; absence is proven only from complete evidence.
- **Scheduled work.** Checks anchor on the time the work was scheduled and
  observe early and late. A missed window is inconclusive. Clocks are never moved.
- **Source.** Snapshots exclude logs, pid files, installed packages, and build
  output. The store must start from the saved source alone.

## Production check families

| Family | What it requires |
| --- | --- |
| `state-durability` | Saved data survives a restart and a fresh sign-in; sessions survive a reload. |
| `access-control` | Customer data and staff operations are refused to the wrong caller, through direct server calls as well as the page. |
| `live-state` | Shared views reflect other users' changes without a reload. Polling that meets the window counts. |
| `concurrency-safety` | Several customers can act at once without overselling, lost orders, or double counting. |
| `external-data-sync` | A direct database write to stock shows in the store. |
| `transactional-integrity` | Stock and money are never created or lost by a partial operation. |
| `cancellation-queue`, `cancellation-accounting` | A cancelled order leaves the queue and reverses exactly its own effects, once. |
| `price-accounting`, `price-history` | Price edits do not rewrite earned revenue; receipts keep the agreed price. |
| `inventory-conservation` | A transfer moves stock without changing the total; an overdraw has no partial effect. |
| `operations-access`, `review-access`, `deferred-access` | Staff, review, and scheduling operations follow the product's roles on the server. |
| `deferred-durability`, `deferred-integrity`, `server-time` | Scheduled work survives a restart, runs once, and runs on time with the browser closed. |

These checks cover the declared product behaviors. A score is not "production
readiness"; it says nothing about accessibility, operations, performance, or
security beyond the listed boundaries.

Notes on individual checks:

- **Shipping** waits for the declared submission state, then reads fresh staff
  and customer views; it does not require a live queue update.
- **Stock alerts** require a confirmed alert request before the first restock.
  The duplicate-alert check samples a fresh client ten seconds after the second
  restock; it does not watch continuously.
- **Staff roles (621b, 621d)** request a role change through HTTP and reducer
  replay and reload the administrator view. They do not test token revocation.
- **Purchase session (101a)** proves a direct purchase works signed in, then
  requires refusal with no session, after rejected sign-ins, with a tampered
  session token, and from an untrusted origin, with stock and orders unchanged.
  A probe that cannot be built, such as mixed cookie and bearer credentials, is
  inconclusive.
- **Checkout crash** kills the application and database processes separately
  on PostgreSQL and MongoDB, and the single combined process on SpacetimeDB,
  Convex, and Supabase. A missed fault window is inconclusive.

## What the checks do not prove

| Observation | Does not establish |
| --- | --- |
| Restart with retained data | Database crash recovery, power loss, or corruption recovery |
| Checkout interrupted by a process kill, then reconciled | Power loss, disk corruption, every crash timing, or external payment durability |
| Private marker absent from captured responses | Every endpoint, encoding, or object-reference attack |
| Exact final stock, orders, and totals | Every intermediate state, general serializability, or an external ledger |
| Bounded concurrent requests | Sustained throughput or overlap inside the server |
| Serial promotion redemption limit | A race for the last redemption |
| Hidden return and activity controls | Server-side return authorization or audit-log confidentiality |

## Review a failed check

A failed check records an observation that did not meet an assertion. It does
not identify a bug or its cause, and several checks can fail from one missing
update path or one failed setup step.

For each failure you investigate, keep a short note beside the evidence: the
campaign, execution, source hash, and check IDs; the observed result, separate
from the proposed cause; whether the evidence shows an application defect, a
harness defect, or neither; and what evidence would settle it. Group checks
only when evidence shows a shared cause, and never report the group count as a
bug count.

Stay inside the measured boundary. A button shown to a guest proves a
visibility failure only if the contract forbids it; it does not prove the
server accepts a guest purchase. A missing stock number does not prove
overselling. Use direct-call results and stored quantities for those claims.

If the review finds a grader defect, keep the original artifacts, fix the
grader, and regrade the saved store into separate evidence with the
[regrade command](../appliance/README.md#regrade-a-saved-attempt). A regrade
is not a new build, and it cannot reconstruct the different path a dependency
run would have taken under the corrected gate.
