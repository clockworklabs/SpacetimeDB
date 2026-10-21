# Timing windows that the prompts do not state

A review of every timing-dependent L1-L3 step found windows tighter than designs the
prompts allow. Changes (85b118753e):

- 631a, 631b, 631c (stock alerts): delivery now gets 75 s after the restock plus 15 s
  on a fresh sign-in, as 631d does, so a once-a-minute sweep passes all four. The
  631a duplicate sample also waits 75 s after the second restock.
- 3b (buy-now live stock): its other-client reads had no `within` and got the 5 s
  default; they now get the 10 s every other live check uses.
- 201a (last unit): the six clients reload before their stock is read. The check
  measures stock integrity; live propagation has its own checks.

Kept as is by decision: live-update checks stay at 10 s ("live" means seconds),
including 613a, where two apps poll support cases every 20 s.

No counted run failed any of these checks on timing alone, so no score changes.

Diagnostic evidence on image 85b118753e, `results/diagnostics/timing-fixes-20261021`:
references 9/9 on the changed checks on all five stacks, and 29 of 29 seeded defects
targeting them caught. The stock alert pack now measures up to 113.1 s (SpacetimeDB),
so its budget is 227 s; the other affected packs stay within their budgets.
