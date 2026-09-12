# Bundle checkout interface

Use `bundle-add-to-cart` inside `bundle-card`. A bundle cart line uses the existing
`cart-item`, `cart-reservation-timer`, and `cart-item-expired` interfaces, with
`bundle-remove` to remove it. `checkout-submit` buys the cart through the existing checkout
action. The existing `order-item` and `payment-amount` show the bundle name and price paid.

<!-- interface:http -->
Add one bundle with `POST /api/cart/bundles` and `{ bundleId }`.
<!-- /interface -->

<!-- interface:reducer -->
Add one bundle with `add_bundle_to_cart(bundleId: u64)`.
<!-- /interface -->

Use the same application action as the visible control. The `data-bundle-input` attribute
supplies its bundle ID. The cart and order interfaces remain shared with individual products.
