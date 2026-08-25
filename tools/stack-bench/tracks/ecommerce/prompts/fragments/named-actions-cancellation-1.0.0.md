## Testing call for cancellation

Cancellation is already part of the requested product work. Expose the same server write through the testing call below. Do not add another transport only for Stack Bench.

| Action | MongoDB/PostgreSQL app | SpacetimeDB app |
|---|---|---|
| cancel an order | `POST /api/orders/:id/cancel` | reducer `cancelOrder` |

The same authentication, ownership, and data rules apply to this call.
