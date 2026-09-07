# Order cancellation application interface

Use `order-status` for an order's state inside its `order-item`. Use `cancel-order` on a pending
order. Use `catalog-link` to return to the catalog.
`order-status` reads `pending` until the order ships, `shipped` once it has, and `cancelled`
after a cancellation. Later features may add further states after `shipped`.

<!-- interface:http -->
Use `POST /api/orders/:id/cancel`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `cancel_order` reducer.
<!-- /interface -->
