# Bundle return interface

Use `return-bundle` inside the existing `order-item`. Its `order-status` is exactly
`returned` after return. `bundle-refund-amount` shows the refunded amount in currency units.
Each bundle `order-item` exposes `data-bundle-return-input` as JSON `{ "orderId": "..." }`.

<!-- interface:http -->
Return a whole bundle with `POST /api/bundle-orders/:orderId/return`.
<!-- /interface -->

<!-- interface:reducer -->
Return a whole bundle with `return_bundle(orderId: u64)`.
<!-- /interface -->

Use the same application action as the visible control.
