# Level 3 application interface

## Reservation hooks

| Element ID | Element |
|---|---|
| `cart-reservation-timer` | remaining reservation time in seconds, inside `cart-item` |
| `cart-item-expired` | expired marker inside `cart-item` |
| `cart-expired-notice` | notice shown after cart expiration |

## Scheduled restock hooks

| Element ID | Element |
|---|---|
| `schedule-restock-item` | item input |
| `schedule-restock-warehouse` | warehouse input |
| `schedule-restock-qty` | quantity input |
| `schedule-restock-delay` | delay in seconds input |
| `schedule-restock-submit` | schedule button; set `data-action-input` to a JSON object with `item`, `warehouse`, `quantity`, and `delaySeconds` |
| `pending-restock-item` | pending row with `data-entity-id` |
| `pending-restock-remaining` | remaining time in seconds inside the pending row |
| `pending-restock-cancel` | cancel button inside the pending row |
| `stock-ledger-entry` | completed stock movement row |

## Order delivery hooks

| Element ID | Element |
|---|---|
| `completed-order-item` | completed order in the staff view |
| `completed-order-status` | current state inside `completed-order-item` |
