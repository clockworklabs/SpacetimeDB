# Fulfilment application interface

| Element ID | Required element |
| --- | --- |
| `staff-link` | Opens the fulfilment area. |
| `queue-depth` | Shows the number of pending orders. |
| `queue-item` | Shows one pending order and names its items. |
| `queue-warehouse` | Shows the selected warehouse inside its `queue-item`. |
| `ship-submit` | Marks the order in its `queue-item` as shipped. |

Each customer `order-item` must have these attributes:

- `data-ship-input` contains a JSON object with exactly `orderId`.
- `data-cancel-input` contains a JSON object with exactly `orderId`.

Use the identifier representation required by the selected stack.

<!-- interface:http -->
Use `POST /api/fulfilment/ship` with `{ "orderId": ... }`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `ship_order` reducer.
<!-- /interface -->

<!-- interface:http -->
Use `POST /api/orders/:id/cancel`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `cancel_order` reducer.
<!-- /interface -->
