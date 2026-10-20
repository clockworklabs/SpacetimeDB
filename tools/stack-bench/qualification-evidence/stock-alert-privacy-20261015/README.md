# Stock-alert privacy delivery check

A Gemini 3.8 Flash SpacetimeDB app (job `f5f75f0e`, run 3) published its
`stock_alert` table to every client and filtered it by account in the browser.
Check 631b passed because the other customer's screen showed nothing. The new
`markDelivery` and `expectNotDelivered` steps compare the records each client
receives, so that app now fails.

This is a diagnostic overlay image, not a release build or formal qualification.
Image `sha256:cbef3fd537334dc9b5d63d4d108e9d7deafc34bb2f01155cf5a7dd610a3e1251`
was built from `stack-bench-controller:263ec8de58` with the working tree on top
of `4beec959ac`. Regrades used dependency volume
`stack-bench-release-deps-263ec8de58`, or the run's own volume where its start
script regenerates bindings. No model calls were made.

## Results

- `leaderboard-compare.txt`: all 24 leaderboard runs that reach 631 were
  regraded on checks 631a, 631b and 631c. 71 of 72 statuses match the original
  grade. The only change is the Flash app's 631b, from passed to failed with
  `message-delivered` for `other-fresh`.
- `argon-compare.txt`: the six Gemini 4 Argon SpacetimeDB and Convex runs that
  reach 631 (job `c4413fabe4ff`). All 18 statuses match.
- `measurements.txt`: what each step identified. Convex apps sent the owner 3 to
  8 new records on the request. SpacetimeDB apps sent one row or none. Seven
  SpacetimeDB apps sent nothing new, so the step reports `measured: false` and
  only the visible check applies.
- `controls.txt`: the three existing stock-alert seeded defects on each of the
  five reference apps. All 15 were caught by their intended check, and every
  reference baseline passed.

The step brackets only the alert request. A delivered alert that reaches other
clients and is hidden by their UI is not measured by it.
