# Deep seeded defects and reference fixes

24 scored checks at dependency depth 4 to 6 had no seeded defect on any stack
(`uncovered-checks.txt`). This adds one per check on the SpacetimeDB, PostgreSQL
and MongoDB references where a clean defect exists, fixes the references where
they failed those checks, and fixes scenarios that no correct app could pass.

This is diagnostic evidence, not formal qualification. Runs used per-stack
overlay images of `stack-bench-controller:604a507690` with the working tree on
top, and a per-stack copy of the L6 calibration whose checks were widened to the
new targets. The committed L6 calibration still covers only its return and
refund checks: a full L6 calibration needs a defect for every deep check on
every stack, which is not yet the case. No model calls were made.

## Results

Each `<stack>.txt` lists the reference result on the selected deep scenarios,
every new defect's outcome, and a reference-only level 3 grade.

- SpacetimeDB: reference 50/50 on the selected deep checks; 22 of 22 defects
  caught at their target; level 3 176/176.
- PostgreSQL: reference 34/34; 16 of 16 caught; level 3 176/176.
- MongoDB: reference 44/44; 20 of 20 caught; level 3 176/176.

Checks without a defect on a stack: 501a and 504a everywhere (the next check
in the same scenario repeats every observation they make, so no defect can fail
them alone), and on PostgreSQL 403a, 403b, 504b, 503b, 614a and 614b, written
before the reference fixes; they are candidates for the next pass.

## Reference fixes

- SpacetimeDB: unrestored expired-cart lines no longer carry `cart-item`; a
  linked support order names its items; `my_payments` rows carry the payment
  id. Without the id, two equal refund records arrived as one client row, and a
  repeated payout was invisible.
- PostgreSQL: the expired-cart notice shows on the page; staff reach
  `reorder-link` after sign-in; recommendations drop purchased and dismissed
  items; the partial restore names unavailable items; a linked support order
  names its items.
- MongoDB: the `recommendations` hook exists and recommendations drop
  purchased items.

## Scenario fixes

- 615c: a second customer's case is refunded by staff first, so the owner's
  refused replay measures authorization; order status is compared ignoring case,
  as in 36 of the 43 other status checks.
- 615b: the owner closes the orders panel before opening support; refund
  entries are counted inside their order, and an unrefunded order need not show
  a zero refund total.
- 614b: the other customer's attempt runs while the owner's case is unlinked,
  since an app may hide the attach control once an order is attached.
- 403a, 403b: the list order is each named item's `recommendation-rank` in a
  list of three; whole-row text includes the rank the contract requires.
- 501a, 501b: delivery notifications are counted by "deliver", not the item
  name, which no contract requires.
- 624a: the activity action must be present; its wording is not specified.
- 304a: the reopened customer signs back in when the app keeps its session in
  `sessionStorage`.

A named action's `input` may now name the container that holds its control
(`in`), for pages that show one such control per ticket.
