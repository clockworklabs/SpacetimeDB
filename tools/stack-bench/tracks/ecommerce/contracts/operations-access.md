# Fulfilment application interface

| Element ID | Required element |
| --- | --- |
| `staff-link` | Opens the fulfilment area. |
| `fulfilment-panel` | Contains fulfilment tools, including an empty queue; not a loading or error message. |
| `queue-depth` | Shows the number of pending orders. |
| `queue-item` | Shows one pending order and names its items. |
| `queue-warehouse` | Shows the selected warehouse inside its `queue-item`. |
| `ship-submit` | Marks the order in its `queue-item` as shipped. |

On `fulfilment-panel`, expose `data-submit-state` for the latest shipping submission:
`idle` initially, `pending` immediately when submitted, `succeeded` only after the server
confirms success, or `failed` after rejection or transport failure. Keep the terminal state
on the panel when the shipped row disappears. A new submission must replace the old state.

`order-status` reads `pending` until the order ships, `shipped` once it has, and `cancelled`
after a cancellation. Later features may add further states after `shipped`.

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
