# Stock alerts after a direct stock write

New production check `ecommerce.spec.external-data-sync.external-stock-alert.631d`
(1 point, L3): a customer requests an alert for an item with no stock, stock is
then written directly to the database outside the application, and a fresh
sign-in must show the delivered alert within 90 seconds (75 s wait, then 15 s on the
fresh sign-in; originally about 20 s, see below). Its statement comes from
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

## Longer window (2026-10-19)

A code review of the saved apps that failed found two Convex apps that deliver
pending alerts from a once-a-minute cron and failed only because the check gave up
after about 20 seconds. No latency is stated, so the window is now 75 s after the
write plus 15 s on a fresh sign-in (3818a2a875). On image 3818a2a875
(`results/diagnostics/direct-alert-window-20261019`): references 3/3 on the pack's
three L3 checks on all five stacks, and 5 of 5 defects caught. The pack now measures
up to 123.0 s (SpacetimeDB), so its budget is 247 s (b611e81990).
