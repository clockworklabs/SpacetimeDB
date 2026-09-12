# Probe fixes and campaign impact

Campaign: `c12719bf8c915901d06b7ffc4903c1ada47c50953839bdb4cc38d489a4242e01`.
Audit date: 12 September 2026. No saved application or original result was changed.

## Result

Eight attempts produced an L3 grade with 109 checks each. SpacetimeDB 2 has no
completed L3 result and remains excluded from a full-run comparison.

Of the **872 original final L3 check outcomes**:

- **42 failures cannot establish the claimed production defect.** Each stopped on
  a stale view or required a live update before the target could be measured.
- **48 passes need stronger checks.** The old probes can miss a defect. This is
  not evidence that those applications contain the defect.
- **782 outcomes are outside these identified flaws.** This does not certify
  every application behavior or qualify the full benchmark.

| Attempt | Recorded pass count | Failures with a faulty observation path | Passes needing stronger checks |
|---|---:|---:|---:|
| SpacetimeDB 1 | 109/109 | 0 | 6 |
| SpacetimeDB 3 | 108/109 | 0 | 6 |
| MongoDB 1 | 88/109 | 7 | 6 |
| MongoDB 2 | 89/109 | 7 | 6 |
| MongoDB 3 | 88/109 | 7 | 6 |
| PostgreSQL 1 | 89/109 | 7 | 6 |
| PostgreSQL 2 | 87/109 | 7 | 6 |
| PostgreSQL 3 | 87/109 | 7 | 6 |

These are check counts, not weighted points. The saved-app recheck below supplies
new measurements for the affected keys. It does not replace the original scores.

## Which checks

All six MongoDB/PostgreSQL attempts failed these observations in their final L3 grade:

| Stable check key | Recorded failure |
|---|---|
| `ecommerce.spec.concurrency-safety.last-unit.201c` | Existing admin revenue stayed at zero. |
| `ecommerce.spec.access-control.purchase-session.101a` | Purchase stock setup used an old catalog view. |
| `ecommerce.spec.concurrency-safety.restock-race.202a` | Restock setup used an old stock view. |
| `ecommerce.returns-pricing.refund-accounting.203a` | Existing admin revenue stayed at zero. |
| `ecommerce.l3.deferred-durability.restart-survival.311a` | The ordinary restock prerequisite used an old stock view. |
| `ecommerce.l3.server-time.server-time.312a` | The final stock view did not receive the scheduled update. |
| `ecommerce.spec.transactional-integrity.books-balance.107a` | Existing admin revenue stayed at zero. |

All eight completed attempts passed these six checks with weaker evidence:

| Stable check key | Missing evidence |
|---|---|
| `ecommerce.operations-access.order-owner.204a` | Fresh owner state after a refused outsider cancellation. |
| `ecommerce.inventory-operations.warehouse-transfer.2a` | Selected-product stock in each warehouse. |
| `ecommerce.operations-access.operator-authorization.201a` | Selected-product stock in each warehouse after refusal. |
| `ecommerce.inventory-operations.stock-conservation.202a` | Selected-product stock in each warehouse. |
| `ecommerce.spec.transactional-integrity.stock-transfer-overdraw.2c` | Stored stock in each warehouse after overdraft refusal. |
| `ecommerce.spec.live-state.stock-transfers.2b` | Selected-product stock in addition to the displayed warehouse totals. |

The wrong-product transfer weakness was reproduced with the real browser grader:
moving Desk Lamp stock instead of Espresso Machine stock satisfied the old aggregate
assertions. The new product-and-warehouse assertions reject that defect.

The two SpacetimeDB restock timing passes remain supported. Their fresh early reads
occurred 74,269 ms and 74,318 ms after scheduling and showed unchanged stock.
Their later reads showed the exact increment at 120,263 ms and 120,081 ms.
The declared delay was 120,000 ms. A new elapsed-time guard protects slower future
restarts; its addition does not invalidate these observed timing results.

## Earlier levels and the excluded attempt

The artifact audit covered 1,376 check outcomes across all recorded levels:
81 at L1, 423 at L2, and 872 at L3. Repeated grades of a check on different saved
builds are separate observations.

- Six L2 purchase-session failures have the same stale-view setup defect.
- SpacetimeDB 2 failed signout at L1 because the probe did not open its account menu.
  Its L2 grade covered 23 checks instead of the 50 graded in the other attempts.
  Its development path was therefore different. The process was later stopped
  with an incomplete artifact. Keep this attempt excluded.
- Across all levels, 49 outcomes have an identified observation/contract error,
  and 48 passes need stronger checks. No original record was deleted or rescored.

## Changes

- Product-specific warehouse assertions for transfer, overdraft, and access checks.
- Fresh reads for accounting, ownership, reservation, credit, and refund observations.
  Dedicated live-update probes retain their live observers.
- Original-time anchors and early/late observations for deferred work. A missed
  observation window is unmeasured, rather than an invented app failure.
- Separate return-button and direct pending-return checks, with a successful return
  control and fresh stock/accounting observations.
- Reorder baselines, item/quantity checks, accepted purchases, and changed-value
  access attempts. Automatic restock timing now has a defined product deadline.
- Recovery checks require an expired empty cart and renewed available-stock use.
  Delivery notifications require a delivered order and no earlier matching notification.

The new pending-return check and the later-level changes were not in this campaign's
selected L3 checks. They do not create extra historical failures or denominator entries.
The source definitions remain draft pending matching reference and defect-control
qualification. Focused checks do not supply that qualification.

## Use of the data

Keep the original scores, costs, source identities, and evidence. Withhold the full
campaign comparison as a verified result. The common subset outside the 13 affected
check IDs has 96 checks per completed attempt; any analysis of that subset must be
labelled as an audit-selected subset, not the original primary result.

The affected measurements were rechecked on unchanged saved apps in separate diagnostic
artifacts. This tests the saved implementation. It cannot reconstruct development
work that a corrected earlier gate would have requested. A fresh run is required
for that full development-path comparison, including a replacement for SpacetimeDB 2.
No paid rerun was started for this audit.

The counts come from each saved grade's `recipeRelease.checks` and criterion evidence,
not today's graph or dashboard labels. Local audit extracts are retained in
`local-notes/r5-check-evidence.json`, `r5-check-catalog.json`, `r5-impact.json`, and
`r5-restock-timing-audit.json`. They contain the per-attempt mapping behind this report.

## Saved-app recheck

All **104/104 selected check outcomes passed**: the 13 affected keys on each of
the eight completed L3 source snapshots.

| Stack | Rep 1 | Rep 2 | Rep 3 |
|---|---:|---:|---:|
| SpacetimeDB | 13/13 | Excluded | 13/13 |
| MongoDB | 13/13 | 13/13 | 13/13 |
| PostgreSQL | 13/13 | 13/13 | 13/13 |

This includes all 42 earlier failures with faulty observation paths and all 48
passes that needed stronger probes. The other 14 observations repeat the seven
previously passing keys on SpacetimeDB 1 and 3. The revised checks found no defect
in this selected scope. This does not establish that the saved apps have no defects.

All eight regrades used controller revision `486b8aa05` and the original build
image. Their receipts and grade bundles were audited for the original source and
run hashes, exact selected check keys, matching engine identities, bundle hashes,
completed cleanup, and absence of harness failures. The source remained unchanged.
There were **zero model calls** and no additional model cost.

Do not discard the saved apps or buy new builds just to recover these measurements.
Keep this as a diagnostic result. A new campaign is needed to measure the complete
development path under the corrected grader; SpacetimeDB 2 still needs replacement.
No new paid run was started.

Evidence is under the state volume's
`results/diagnostics/probe-regrade-486b/<original-attempt-id>/` directories.
Each contains `regrade.json` and `grading/bundle.json`. The consolidated receipt
audit is `results/diagnostics/probe-regrade-486b-audit.json`, with a local copy at
`local-notes/probe-regrade-audit.json`.

## Control validation

The selected checks were exercised against working references and deliberately
broken versions. Each baseline covered the 13 affected keys plus the two other
last-unit checks needed by the overselling controls.

| Stack | Controller revision | Reference checks passed | Targeted defects caught |
|---|---|---:|---:|
| SpacetimeDB | `25e5c422b` | 15/15 | 12/12 |
| PostgreSQL | `486b8aa05` | 15/15 | 12/12 |
| MongoDB | `7a3304fc3` | 15/15 | 12/12 |

The 36 defect results contain measured failures of their target checks. None has
a missing target, setup failure, harness failure, inconclusive result, or unrelated
failure. The recorded failures were inspected, not just the aggregate counts.
They cover incorrect stock and revenue, unauthorized actions, missing live updates,
and lost or mistimed scheduled work. Early execution was tested on SpacetimeDB;
the MongoDB and PostgreSQL timer defects prevent execution.

Validation exposed and fixed four issues:

- `25e5c422b`: the SpacetimeDB reference client omitted the restock quantity field
  from its local TypeScript type.
- `7fd8b2523`: an optional navigation observation could exhaust its deadline and
  throw a fatal scroll timeout. Required clicks and aborts still fail normally.
  The focused browser-action tests passed. None of the 1,376 original outcomes
  contains this failed scroll observation, so the historical impact counts do not change.
- `486b8aa05`: PostgreSQL reference startup reapplied its core schema and deleted
  extension data, including scheduled work. Core schema setup now runs only for an
  empty database. The live restart and timer controls pass.
- `7a3304fc3`: the MongoDB reference treated missing or foreign orders as input
  errors. Cancellation and return now return 404 before changing stock or refunds.
  The ownership control, server type check, and reference contract checks pass.

Failed validation attempts remain separate evidence. No saved campaign app was
patched to obtain these results. Calibration inputs were refreshed without adding
qualification evidence. These are scoped, single-repetition diagnostics at the
listed revisions. The SpacetimeDB control predates the optional-navigation fix;
the two other stacks exercise that fix. This does not qualify a full current L3 release.

Control artifacts are under the state volume's `results/diagnostics/` directory:
`probe-controls-25e5-r1-spacetime.json`, `probe-controls-486b-r1-postgres.json`, and
`probe-controls-7a33-r1-mongodb.json`. Each links its baseline, worker artifacts,
and individual defect evidence. `results/diagnostics/probe-controls-audit.json`
records the source identities and inspected failures, with a local copy at
`local-notes/probe-control-audit.json`.

## Live-observation ordering review

A follow-up review found that the new stored-stock reads preceded the live transfer
assertions. Slow database reads could therefore give the UI extra time. The reads
now follow both live assertions, preserving their original observation order and
waits. A focused contract check protects this ordering; scenario and calibration
validation pass. This edit does not promote qualification evidence from older revisions.

The eight saved regrades already observed both live totals within 630–1,775 ms of
starting the transfer click. All were within 10 seconds, including the database-read
time. These passes did not depend on the extra time that the ordering could allow.
The action-timestamp audit is retained at
`results/diagnostics/probe-transfer-timing-audit.json`.
