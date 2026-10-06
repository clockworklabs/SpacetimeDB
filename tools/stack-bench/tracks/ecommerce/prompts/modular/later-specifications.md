# Expected production behavior

## Product bundles

Only authorized staff can change bundles.

## Bundle checkout

Reserve all components or none. Competing purchases share the same stock. Release component reservations when the cart expires or the bundle is removed.

## Bundle returns

Return the original component allocations and price paid. Repeating a return must not change stock or refunds again. Customers cannot return another account's order.

## Store credit

Only authorized staff can grant credit. Repeating a reference must not issue credit twice. Concurrent checkout must not duplicate the order or credit use. Accepted credit survives a backend restart.

## Split-tender refunds

Concurrent or repeated refunds must restore each original payment portion only once. The resulting balance and refund records survive a backend restart.

## Scheduled purchases

Pending deliveries and pauses survive a backend restart. Process elapsed pending slots after recovery. Completed delivery slots must not run again. Customers cannot change another account's subscription.
