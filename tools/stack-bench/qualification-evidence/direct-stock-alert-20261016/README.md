# Stock alerts after a direct stock write

New production check `ecommerce.spec.external-data-sync.external-stock-alert.631d`
(1 point, L3): a customer requests an alert for an item with no stock, stock is
then written directly to the database outside the application, and a fresh
sign-in must show the delivered alert within 20 seconds. Its statement comes from
two stated rules: alerts fire when stock returns, and the `stock` table is the
source of truth for corrections made outside the application.

This is diagnostic evidence, not formal qualification. No model calls were made.
Outputs are under `results/diagnostics/direct-alert-20261016` in the state volume.

## References

SpacetimeDB, PostgreSQL and Convex now also deliver alerts from the stock rows on
their existing timers (a cron on Convex); Supabase uses a trigger on stock
increases; MongoDB already delivered from a timer. The PostgreSQL reference now
starts its timers after its schema exists: with the extra query, the timer could
deadlock against startup DDL.

- Targeted run (`reference/`, image c63b0ee10c): 12/12 on the stock-alert,
  external-sync and stock-race checks on all five stacks.
- Full L3 timing run (`timing/`, image d1b3db0eab): 177/177 on all five stacks.
  SpacetimeDB exceeded the external data sync pack budget (57.4 s of 54 s).

The budget is now 115 s: twice the largest measured runtime, rounded up. The
budget tool refuses runs that exceeded the old budget, so its policy was applied
by hand to the five timing runs (SpacetimeDB 57361, MongoDB 52173, PostgreSQL
51527, Supabase 50985, Convex 49677 ms).

## Seeded defects

`stock-alert-misses-direct-stock-writes` on every stack delivers alerts only
through the application's own restock paths. On image ea93b4f029 (`defects/`)
each reference passed 631d and each defect was caught at 631d: 5 of 5.
