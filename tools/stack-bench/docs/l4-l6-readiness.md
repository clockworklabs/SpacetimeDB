# Dependency depths L4–L6

Implementation review: 2026-09-10, based on commit `90818215c` plus the current
working changes. This document is not qualification evidence.

## Accepted scope

Keep L1–L3 product work and dependencies unchanged. Keep returns at L5. Add six
features through the existing packs, contracts, scenarios, and reference apps.
Company accounts and purchasing approvals are not part of this change.

| New feature | Depth | Product parents |
| --- | ---: | --- |
| Product bundles | 4 | Catalog management |
| Bundle checkout | 5 | Product bundles, reservations |
| Store credit | 5 | Payment records, staff roles |
| Subscriptions | 5 | Payment records |
| Bundle returns | 6 | Bundle checkout, returns |
| Split-tender refunds | 6 | Store credit, support refunds |

| Depth | Previous features | Current features |
| --- | ---: | ---: |
| L1 | 4 | 4 |
| L2 | 10 | 10 |
| L3 | 13 | 13 |
| L4 | 9 | 10 |
| L5 | 6 | 9 |
| L6 | 1 | 3 |
| Total | 43 | 49 |

The compiler derives depth from product dependencies. Check prerequisites can
require additional features without adding product edges. The generated
[dependency graph](dependency-graph.html) is the current graph view.

L6 now covers cart recovery, bundle returns, and split-tender refunds. Equal node
counts would not imply equal difficulty or cost. Report reached, passed, failed,
and blocked work separately. Keep metric definitions fixed before collecting data.

## Implemented measurement changes

- Return completion uses an exact status. It first proves that the sale changed
  stored stock and revenue, then checks restoration and a fresh customer view.
- Payment records require an exact paid status. The word “unpaid” cannot pass.
- Delivery notification checks wait for a loaded panel before testing absence.
- Bundle checks cover shared component stock, refused partial reservations,
  expiry, saved component allocations, and direct authorization.
- Credit checks cover grant replay, unauthorized grants, one shared-cart checkout
  race, and persisted credit after restart.
- Split refunds check exact original credit and external amounts, concurrent
  refund requests, and persisted results after restart.
- Subscription checks cover scheduled order/payment counts, stock consumption,
  restart, owner cancellation, and pause/resume across restart.

The current full catalog has 180 checks, including 21 added by these six features.
Every added check has a declared defect target for all three stacks.

These are executable definitions with reference implementations for each stack.
They are draft until matching positive and defect evidence passes. A compiled
scenario or passing source build does not establish live grading correctness.

## MongoDB runtime

Future MongoDB runs use a local single-node replica set, named `rs0`, with
application authentication and a private per-attempt database. This permits
multi-document transactions and change streams. It does not test replica failover
or multi-node availability. The MongoDB adapter version changes with this runtime.

The running campaign retains its frozen standalone runtime. Do not pool its
results with the new configuration. Prior qualification evidence remains tied to
its original source and runtime identities.

## Claim limits and remaining work

- A shared-cart race is not a general wallet overdraft or throughput test.
- Local payment records do not prove correct external payment processing.
- Bundle scope excludes nested bundles and multiple reserved instances of the same
  bundle in one cart. Remove and re-add a bundle to replace its reservation.
- A partial bundle return from a mixed-item order with credit or a discount is
  refused. The reference offers a full support refund for that order.
- Subscriptions accept individual catalog items; bundles are excluded.
- The current subscription restart case does not establish recovery after a long
  outage with many missed periods. Add that control before making the claim.
- The existing L4–L6 catalog still has unqualified checks and missing defect
  controls. The review below is the remaining work list, not completed evidence.
- Preserve intentional TypeScript server, client, CLI, and dev guidance. Compare
  the complete supplied stack package. Do not tune checks until a preferred stack
  wins, or use its success rate alone to decide whether a workload is rigorous.
- Keep first-build, repair, and resumed results distinct. Later work can benefit
  from prior repair feedback. It is not a new experiment from zero.

## Completed focused checks

- Native MongoDB authentication, transaction commit/rollback, process restart,
  persisted data, database reset, and exact container cleanup passed.
- All three reference backends and clients compiled. Clean package installation
  was corrected for the PostgreSQL and SpacetimeDB reference locks.
- Real credit checkout and refund actions passed on all three stacks: grant
  replay, exact credit/external split, original-credit restoration, and no credit
  increase after a repeated refund.
- Real scheduled deliveries produced the expected ordinary orders and payment
  amounts on all three stacks. Runtime checks caught and corrected PostgreSQL
  parameter typing and a stock-key query error; the failed transactions rolled back.
- MongoDB and PostgreSQL bundle smoke checks covered catalog writes, rejected
  customer writes, component reservation, checkout, changed definitions, return
  of the paid amount, and refusal of a repeated return.
- MongoDB and PostgreSQL recovery smoke checks aged only the scratch cart
  timestamps, then changed the bundle definition. Recovery preserved the original
  price and component quantities. This is not a real five-minute timing run.
  SpacetimeDB recovery passed schema generation and type checking.
- Scenario validation and full-depth prompt-boundary checks passed. Mutation
  anchors and syntax were checked, but the new defect controls were not executed.

Declared controls include lost credit and pending work, but they have not been
executed. They do not yet establish restart qualification for the new checks. The subscription smoke verifies ordinary execution;
its restart, cancellation, and pause probes still require live qualification.

## Validation sequence

1. Validate graph, selected contracts, scenarios, reference builds, and mutation anchors.
2. Run focused model-free behavior checks for the changed money and timer paths.
3. Qualify changed scopes with correct references and targeted defect controls.
4. Freeze matching source, runtime, definitions, and evidence before comparing a new cohort.

No new paid campaign or full qualification run is part of this implementation.
Current evidence must not be relabeled after definitions or reference sources change.

## Review of every later feature

These are source findings from the baseline and proposed checks. The exact-status,
stored return accounting, and notification-readiness fixes above are now implemented. A proposed race or restart case
still needs a valid reference and a defect control before it supports a claim.

| Depth / feature | Current evidence or gap | Preparation |
| --- | --- | --- |
| L4 Price history | Checks live prices, paid-price preservation, revenue, direct authorization, and cart checkout. | Add a price-change/checkout race. Accept a consistent permitted price; reject mixed order, payment, and revenue totals. State the price policy before grading. |
| L4 Reservations | Checks holds, expiry, renewal, checkout, and restart. Most stock assertions read the UI. | Reuse stored stock reads. Race checkout against expiry and renewal against the old timer. Check that a sale or release occurs once and a renewed hold is not released by stale work. |
| L4 Order delivery | Checks eventual delivery and one displayed completed order after restart. | Check the allowed transition history and side effects. A single final row cannot prove one execution. Add cancellation/shipping conflict cases and restart while work is pending. |
| L4 Payment records | Concurrent checkout produces one order, one displayed payment record, and an exact amount in a fresh client. | Use exact payment status, durable amount/count observations, and replay after lost acknowledgement. Local records alone do not prove correct external charging. |
| L4 Promotion checkout | Checks one displayed discount and sequential expired/exhausted-code errors. | Race buyers for the final redemption. Inspect persisted orders, discount totals, and usage count. Show that invalid codes cannot change server totals, even through direct checkout. |
| L4 Personalized recommendations | Checks specific ordering and separation of customers' lists. | Add fresh-login persistence and direct ownership checks where account data is exposed. Keep the stated ranking policy; do not grade subjective recommendation quality. |
| L4 Staff activity | Checks one visible actor/action/subject/time entry. Customer access is tested by an absent link. | Test direct reads, persisted history, and actor attribution after role changes. Require a valid timestamp, not just a time element. Test multiple action types before claiming all changes are audited. |
| L4 Order-linked support | Includes a valid owner action, forged other-owner order linkage, refusal, and a fresh view. | Also test another customer's case ID, direct order reads, and persistence. Keep case ownership and order ownership separate. |
| L4 Automatic reorder | Counts one pending row after sequential sales; tests unauthorized rule replay. | Verify the row's item, quantity, destination, and eventual stock effect. Race threshold crossings, restart pending work, and prove the next threshold cycle can schedule again. |
| L5 Returns | Checks stock/revenue returning to baseline and return-button absence before shipping. | First prove the original sale changed stock and revenue. Require exact returned status, a direct premature-return refusal, owner checks, and duplicate/concurrent return accounting. |
| L5 Cart expiration | Waits for expiry, stock release, and an empty cart; includes restart. | Distinguish the 90-second reservation from five-minute cart inactivity. Specify which actions reset inactivity. Test activity near expiry and prevent a stale timer from deleting an updated cart. |
| L5 Promotion reporting | Checks one redemption and one discounted revenue value. | Reconcile several orders and promotions after replay and restart. State whether reports show gross sales or net refunds; do not assume a refund accounting policy. |
| L5 Delivery notifications | Counts one owner notification and no matching notification for another customer. | Require a loaded destination for empty-list assertions. Check disabled preferences, restart, repeated delivery, and persisted count. Do not infer autonomous delivery from a read that may trigger work. |
| L5 Recommendation feedback | Tests dismissal after restart and another customer's unchanged list. | Verify successful list loading and an unrelated positive result before absence. Add direct cross-account mutation and a fresh other-account view. |
| L5 Support refunds | Checks amount, one refund record after serial replay, and customer refusal. | Race two staff refunds and a refund against a return. Prove aggregate refund cannot exceed paid amount. Check order, case, payment, and inventory effects after restart; first specify refund versus restock policy. |
| L6 Cart recovery | Checks restored cart rows and warning text after a five-minute wait. | Prove exact quantities and stored stock deltas. Race two restores and a competing purchase. Replay after restart; test ownership. Partial recovery must preserve available lines without reserving unavailable units. |
