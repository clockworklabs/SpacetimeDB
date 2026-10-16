# Deep seeded defects and reference fixes

24 scored checks at dependency depth 4 to 6 had no seeded defect on any stack
(`uncovered-checks.txt`). This adds one per check on the SpacetimeDB, PostgreSQL
and MongoDB references where a clean defect exists, fixes the references where
they failed those checks, and fixes scenarios that no correct app could pass.

This is diagnostic evidence, not formal qualification. Runs used per-stack
overlay images of `stack-bench-controller:604a507690` with the working tree on
top, and a per-stack copy of the L6 calibration whose checks were widened to the
new targets, until the second round below made the full calibration possible.
No model calls were made.

## Results

Each `<stack>.txt` lists the reference result on the selected deep scenarios,
every new defect's outcome, and a reference-only level 3 grade.

- SpacetimeDB: reference 50/50 on the selected deep checks; 22 of 22 defects
  caught at their target; level 3 176/176.
- PostgreSQL: reference 34/34; 16 of 16 caught; level 3 176/176.
- MongoDB: reference 44/44; 20 of 20 caught; level 3 176/176.

A second round (appended to each `<stack>.txt`) added 23 defects so that every
one of the 74 scored checks at depths 4 to 6 has a defect on all three stacks:
the restart checks 312a, 315a and 316a everywhere, the PostgreSQL checks left
from the first round, and MongoDB 403a and 403b. All 23 were caught, and each
reference passed its affected scenarios (SpacetimeDB 20/20, PostgreSQL 34/34,
MongoDB 26/26). 501a and 504a repeat observations that 501b and 504b make, so
their defects target both checks of the pair and count only when both fail.

With that coverage the L6 calibration now selects the full dependency L6
catalog (186 checks) and is named `ecommerce.dependency-l6-calibration`. It has
not been qualified.

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
