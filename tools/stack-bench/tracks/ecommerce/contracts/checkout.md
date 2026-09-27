# Checkout application interface

Use `checkout-submit` to check out and `buy-error` for a failed checkout. Use `orders-toggle`
to open order history and `order-item` for each order created by checkout.
While signed in, show `orders-toggle` directly or reveal it by clicking `current-user`.
No other navigation is required to reach it.

Expose the same checkout used by `checkout-submit`.

<!-- interface:http -->
Use `POST /api/checkout`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `checkout` reducer.
<!-- /interface -->

<!-- interface:convex -->
Use the `api:checkout` mutation with `{}`.
<!-- /interface -->

<!-- interface:supabase -->
Use the `checkout` PostgreSQL function with `{}`.
<!-- /interface -->
