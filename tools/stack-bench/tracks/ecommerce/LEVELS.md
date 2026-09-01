# Ecommerce progression

The ecommerce track tests shared numeric state, per-account state, access
control, concurrency, and deferred work through a storefront and warehouse
system.

The track supports two run modes. They use the same feature packs and checks,
but order the work differently.

## Sequential mode

Sequential levels are cumulative. A higher level repeats earlier checks so a
regression stays visible.

### L1: Storefront and warehouse

- accounts and sessions;
- catalog, ranking, stock, and ratings;
- carts, checkout, purchases, and order history;
- reviews and warehouse administration;
- ownership, durability, live updates, and bounded contention.

### L2: Business operations

- fulfilment and shipping;
- stock transfers;
- cancellation and returns;
- price history and paid-price preservation;
- inventory, sales, and recommendation views;
- staff authorization and accounting invariants.

### L3: Deferred work

- reservations and expiration;
- scheduled restocks;
- automatic order delivery;
- abandoned-cart cleanup;
- restart durability and exactly-once execution.

The current sequential aliases and candidate recipes are defined in
`composition/promotions.json` and `composition/candidates.json`. Use
`qualification status` for launch and promotion state. This file does not copy
that changing status.

## Dependency mode

Dependency mode follows the feature graph in
`progression/ecommerce-2.0.1.json`. A feature opens only after its declared
parents pass. Failed branches can stop while unrelated branches continue.

Graph depth is calculated from prerequisites. It is not the same as a sequential
release label. The graph includes independent product areas such as identity,
catalog, checkout, inventory, staff access, support, promotions, notifications,
and recommendations.

Run this command to regenerate the public graph:

```bash
npm run graph
```

The generated file is `docs/dependency-graph.html`. Do not edit it by hand.

## Composition and scoring

Features and specifications are separate packs. A campaign chooses the product
work, which specifications appear in the request, and which applicable checks
are scored.

Every check produces `passed`, `failed`, `inconclusive`, or `harness_failure`.
Only passed checks add points. Missing or inconclusive evidence never changes
the declared denominator.

See [composition/README.md](composition/README.md) for pack, recipe, and
calibration ownership.
