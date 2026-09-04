# Checkout application interface

Use `checkout-submit` to check out and `buy-error` for a failed checkout. Use `orders-toggle`
to open order history and `order-item` for each order created by checkout.

Expose the same checkout used by `checkout-submit`.

<!-- interface:http -->
Use `POST /api/checkout`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `checkout` reducer.
<!-- /interface -->
