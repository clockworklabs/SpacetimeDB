# Order cancellation testing interface

Use `order-status` for an order's state inside its `order-item`. Use `cancel-order` on a pending
order.

Expose the same cancellation write through the testing call below. Do not add another transport
only for Stack Bench. The same authentication, ownership, and data rules apply to this call.

| Stack | Testing call |
|---|---|
| MongoDB or PostgreSQL | `POST /api/orders/:id/cancel` |
| SpacetimeDB | reducer `cancelOrder` |
