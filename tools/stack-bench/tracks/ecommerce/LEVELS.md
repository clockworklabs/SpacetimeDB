# Ecommerce track

The ecommerce track is an online store with a warehouse: shared stock, customer
accounts and orders, staff operations, and scheduled work. It tests shared
numeric state, per-account state, access control, concurrency, and deferred
work. What the checks observe is in [grading](../../docs/grading.md); packs,
recipes, and calibrations are in [composition](composition/README.md).

## Dependency mode

The default mode follows the feature graph in `progression/ecommerce.json`
(see **feature** in the [terms](../../docs/system-design.md#terms)). A
feature's depth is how far down the graph it sits, and a campaign targets a
depth:

- **Depth 1**, 4 features that build on nothing: accounts, catalog items,
  staff access, support intake.
- **Depth 2**, 10 features that build on depth 1: catalog discovery, customer
  profile, purchasing and orders, cart, warehouse administration, staff roles,
  support triage, support history, promotion rules, notification preferences.
- **Depth 3**, 13 features that build on depth 2: checkout, reviews,
  fulfilment queue, stock transfers, order cancellation, inventory dashboard,
  sales dashboard, recommendations, scheduled restocks, catalog management,
  faceted search, managed support cases, stock alerts.

The graph continues to depth 6 with price history, reservations, delivery,
payments, promotions, bundles, subscriptions, store credit, returns, and
split-tender refunds. Convex and Supabase have reference apps for depths 1 to
3; SpacetimeDB, PostgreSQL, and MongoDB through depth 6.

## Sequential mode

Sequential mode runs cumulative levels, L1 to L3, and a level must pass before
the next starts. The recipes are `composition/recipes/sequential-l1.json` to
`sequential-l3.json`.
