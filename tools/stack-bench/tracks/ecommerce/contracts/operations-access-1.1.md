# Fulfilment testing interface

| Test id | Required element |
| --- | --- |
| `staff-link` | Opens the fulfilment area. Show it to staff and administrators. Do not show it to customers. |
| `queue-depth` | Shows the number of pending orders. |
| `queue-item` | Shows one pending order and names its items. |
| `queue-warehouse` | Shows the selected warehouse inside its `queue-item`. |
| `ship-submit` | Marks the order in its `queue-item` as shipped. |

Each customer `order-item` must have these attributes:

- `data-ship-input` contains a JSON object with exactly `orderId`.
- `data-cancel-input` contains a JSON object with exactly `orderId`.

Use the identifier representation required by the selected stack.

Expose the ordinary server action named `ship`. Server-based stacks use `POST /api/fulfilment/ship` with `{ "orderId": ... }`. SpacetimeDB uses the `ship_order` reducer.

Expose the ordinary server action named `cancel`. Server-based stacks use `POST /api/orders/:id/cancel`. SpacetimeDB uses the `cancel_order` reducer.
