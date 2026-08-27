# Ecommerce track — level sequence

The ecommerce track measures shared numeric state, per-account state, access
control, and concurrency through a storefront backed by warehouses. Levels are
cumulative: a higher-level recipe must retain the exact checks owned by its
base release.

## Status

| Level | Product scope | Release status |
|---|---|---|
| L1 | storefront, accounts, carts, purchases, reviews, warehouses | `ecommerce.l1-modular@2.5.0` is promoted; current evidence must be refreshed |
| L2 | fulfilment, transfers, cancellations, returns, and pricing | `ecommerce.l2-standard@1.6.0` is promoted; current evidence must be refreshed |
| L3 | reservations and scheduled work | `ecommerce.l3-standard@1.0.0` is a candidate; no qualified release |
| L4 | not yet defined as a cumulative release | design target only |
| L5 | not yet defined as a cumulative release | design target only |

The promotion and candidate catalogs, not this document, are authoritative for
which exact release can launch. Qualification status is computed with
`commands/qualification-cli.mjs status`.

## L1 — Storefront and warehouse

L1 establishes the reusable application base:

- account creation, sign-in, sign-out, and durable account state;
- a public product catalogue with sales ranking, stock, and ratings;
- authenticated purchases, carts shared across sessions, checkout, and order
  history;
- customer reviews and warehouse administration;
- authorization, ownership, server-authoritative pricing, and accounting
  invariants;
- bounded contention, external-data synchronization, reconnect recovery, and
  exact-once updates to an already-open list.

Headline values are derived from shared state: stock is the sum of warehouse
rows, ratings are derived from reviews, rankings are derived from purchases,
and revenue is derived from orders. The grader verifies the relevant values in
multiple views rather than accepting element presence as proof.

The promoted L1 2.5 recipe contains 48 checks worth 58 points. Forty-six checks
are scored. The restock precondition and external-write reload assertion run as
supporting controls with zero points.

## L2 — Running the business

L2 adds staff and administrative operations over the L1 store:

- fulfilment queues and shipping authorization;
- stock transfers with directional and total-stock conservation;
- cancellation, return, and stock restoration;
- price history and paid-price preservation;
- low-stock, category-sales, recommendation, and best-seller views;
- cross-account authorization, refund accounting, and transfer-versus-purchase
  conservation.

The promoted L2 1.6 release rebases these checks onto exact L1 2.5. It retains
all 48 L1 checks and adds 28 L2 checks, for 76 checks and 117 points. Its source-
bound defect definitions cover every scored check on MongoDB, PostgreSQL, and
SpacetimeDB. A new campaign still requires exact reference, mutation, and
null-control artifacts from the current harness.

## L3 — Deferred work

The L3 product target covers reservations, scheduled restocks, order-state
transitions, abandoned carts, and work that must survive process restarts and
execute exactly once. Its recipe, reference apps, and mutation definitions are
candidates. L3 has no accepted qualification evidence or promoted release. It
must not be reported as benchmark data yet.

## Later cumulative releases

The next cumulative recipe has not been selected. The dependency progression
does not force all work into one fixed release sequence.

## Dependency progression

The draft dependency model is defined in
`progression/ecommerce-1.0.0.json`. Its levels are calculated from feature
prerequisites. They are separate from the cumulative recipe labels above.
Run `npm run graph` from `tools/stack-bench` to regenerate the public view at
`docs/dependency-graph.html`.

Dependency Level 4 currently contains cart expiration, order delivery,
order-linked support, personalized recommendations, price history, promotion
reporting, automatic reorder rules, and staff activity history.

Dependency Level 5 currently contains cart recovery, delivery notifications,
returns, recommendation feedback, and support refunds.

The current draft has 39 feature nodes across ten product areas. Its four
independent roots are accounts, catalogue, staff access, and support intake.
Cancellation and returns are separate nodes because they occur at different
stages of the order lifecycle.

Large-data, long-history, and high-concurrency workloads are test profiles.
They measure any applicable feature path. They are not feature nodes added only
to create another level.

## Composition and scoring

Features and specifications are separate modules. A run can select feature
packs, choose which specifications are stated in the initial prompt, and choose
which applicable specifications are evaluated. Dependencies are resolved before
the prompt and check selection are hashed.

Every check has one typed outcome: `passed`, `failed`, `inconclusive`, or
`harness_failure`. Only passed, point-bearing checks add to the score. A missing
or inconclusive measurement never changes the declared denominator. Zero-point
controls remain visible as supporting evidence.

Checks are promoted only after the exact reference passes, the null application
cannot earn credit, and source-bound defects make the intended check fail
without unrelated regressions on every supported stack.
