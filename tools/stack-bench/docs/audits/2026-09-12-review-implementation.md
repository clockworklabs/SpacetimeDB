# Review implementation

This change repairs measurement, runtime, and reporting defects. It does not qualify
the edited definitions or replace historical scores. No paid run is part of this change.

## Measurement

- Ordinary initial navigation timeouts use the same application-failure rule as
  later navigation. Recognized browser and process faults remain harness failures.
- Conditional navigation reads the visible destination without waiting for animation
  stability. An unreadable destination does not justify clicking a toggle blindly.
- Repair stall detection uses the best measured check outcomes in the existing event
  history. First setup recovery counts as progress; repeating an earlier state does not.
- Return checks observe the contracted returned marker inside the matching order item.
  They retain stock and refund assertions. Human-readable status text ignores case;
  machine identifiers and protocol values remain exact.
- Production checks move from feature packs into specification packs. Ordinary
  dependency prompts still omit specification requirements. Explicit specification
  guidance remains a separate selection. The stock-transfer rejection check was
  already selected once; its sequential category changes without removing points.
- Basic recommendation dismissal and persistence are separate checks. Refund/return
  interaction remains requested product behavior, with one feature owner at L6.
- Delivery completion uses fresh views because the delivered modular request does
  not require live delivery. Dedicated live-update checks keep their live observers.

## Runtime and reporting

- Each campaign must specify parallelism. Shared host admission is atomic and counts
  each reserved execution index once. Temporary capacity shortages queue work;
  impossible requests fail clearly. Admission does not rewrite the requested value.
- Controller and child use one claim timestamp. Supervisor errors remain in process
  evidence, including when the child exits zero. An unexplained signal is not labelled
  as an operator cancellation without matching intent.
- Corrupt job records do not stop unrelated dispatch or produce fabricated results.
- Broker budget stops retain measured spend and reservation details. They are provider
  budget failures, not application defects. Account-mode reservations remain conservative
  because the endpoint does not supply a verified enforceable per-request output cap.
  Model and reasoning settings are unchanged.
- Comparison metrics use eligible attempts. Operational progress and incurred spend
  remain visible. Live cost updates use the existing cache without full-state polling.
- Campaign reports omit absent receipt fields instead of serializing `undefined`.
  Retained receipt values are validated before report aggregation. Reports and the
  dashboard share one recorded-spend calculation. An incomplete execution keeps its
  known subtotal while its final total remains unknown.

## Historical use

The [saved-app impact audit](2026-09-12-probe-impact.md) remains scoped evidence:
104 diagnostic rechecks across eight saved applications, not certified replacement
campaign scores. Original apps, requests, receipts, and results are preserved.

| Historical case | Disposition |
|---|---|
| Eight completed L3 attempts in campaign `c12719bf…` | Keep original results and diagnostic regrades separate. The existing audit identifies 42 faulty failures and 48 weak passes; this change does not establish additional app defects. |
| SpacetimeDB 2 in that campaign | Exclude from a complete L3 comparison. The earlier signout probe changed its progression path; a final-app regrade cannot reconstruct that build. |
| Return 3c/3f, delivery 303a, refund interaction 757a/b, recommendation 504c | Later-depth scope. Do not add them to that historical L3 denominator. |
| Historical signal deaths | Retain interruption and known process evidence. Exit 143 alone does not identify the sender or invalidate all completed observations. |
| Broker-budget stops | Incomplete attempts, not completed application scores. Reserved money is not billed spend. |
| Dashboard aggregation defects | Recompute displays from eligible evidence; the display defect alone does not require paid reruns. |

The subsequent read-only durable-record audit found 20 exit-143 executions in the
bounded inventory, rather than the review's 17. Their retained logs confirm SIGTERM
handling but not the sender. In `77c4e7`, the saved cancellation request occurred after
the six signal exits. In `24357e`, the frozen mode has no planned L2 pause.
Passed L2 snapshots therefore do not establish safe paused-run recovery.

Eight allowance failures in `e373af` have $47.829793 in saved cost fields, all marked
incomplete. The PostgreSQL broker stop in `2e1193` has $1.449596 saved and occurred
before that job's later cancellation. These snapshot amounts are not reconciled final
spend or measured harness losses. The exact execution inventory is retained in the
local audit; no historical evidence was edited.

## Qualification still required

All edited calibrations remain draft with no earned qualification evidence. Matching
reference, null, and defect-control executions are separate release work. A successful
full-depth reference does not prove reachability on a lower-depth application. Staged
lower-depth states and pause/resume continuity need their own evidence when used.

The pre-due observation bound remains unchanged: missing that window is inconclusive.
Fetch-based SSE streams are not supported by the current observer and fail closed;
they must not earn a privacy pass. Later-depth reference fixes need live qualification.
The new SpacetimeDB dismissal-loss control proves reconnect coverage, not backend
restart loss alone. Final-cent refund settlement has direct arithmetic coverage on
all three reference implementations; full reference app builds remain a separate check.

## Implementation validation

The combined build passed. The isolated dashboard suite passed all 67 checks.
The initial Linux unit and contract gate had 1,230 passes and 33 failures. Targeted
corrections resolved 32 failures; the remaining cost-report assertion was corrected
with the shared recorded-spend calculation. The final report, cost, checkpoint,
live-metric, and module-layout group passed all 50 checks. The full gate was not
repeated after those focused corrections. No paid campaign or live qualification
was launched for this implementation.
