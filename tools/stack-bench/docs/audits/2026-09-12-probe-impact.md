# Probe fixes and campaign impact

Campaign: `c12719bf8c915901d06b7ffc4903c1ada47c50953839bdb4cc38d489a4242e01`.
Audit date: 12 September 2026. No saved application or original result was changed.

## Result

Eight attempts produced an L3 grade with 109 checks each. SpacetimeDB 2 has no
completed L3 result and remains excluded from a full-run comparison.

Of the **872 final L3 check outcomes**:

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

These are check counts, not weighted points. Do not convert the affected failures
to passes or publish adjusted full scores without new measurements.

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

Recheck the affected measurements on unchanged saved apps in separate diagnostic
artifacts. This can test the saved implementation. It cannot reconstruct development
work that a corrected earlier gate would have requested. A fresh run is required
for that full development-path comparison, including a replacement for SpacetimeDB 2.
No paid rerun was started for this audit.

The counts come from each saved grade's `recipeRelease.checks` and criterion evidence,
not today's graph or dashboard labels. Local audit extracts are retained in
`local-notes/r5-check-evidence.json`, `r5-check-catalog.json`, `r5-impact.json`, and
`r5-restock-timing-audit.json`. They contain the per-attempt mapping behind this report.
