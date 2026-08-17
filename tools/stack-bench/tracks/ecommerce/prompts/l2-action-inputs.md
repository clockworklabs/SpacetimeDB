## Operator action inputs

Expose machine-readable inputs for the public testing interface. These are inert HTML attributes; they do not prescribe the application framework, database API, or internal implementation.

- Each `order-item` exposes `data-ship-input` containing a JSON object with exactly `orderId`.
- Each `order-item` exposes `data-cancel-input` containing a JSON object with exactly `orderId`.
Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in these attributes.

## Price action input

- Each `admin-item-row` exposes `data-price-input` containing a JSON object with exactly `itemId` and numeric `price`. The Gaming Mouse row uses `1.00` as the price so the grader can verify that a non-admin is refused without relying on a visible button.

Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in this attribute.

## Inventory contention action inputs

Expose machine-readable inputs for the public testing interface. These are inert HTML attributes; they do not prescribe the application framework, database API, or internal implementation.

- Each catalog `item-card` exposes `data-buy-input` containing a JSON object with exactly `itemId`.
- The Headphones `admin-item-row` exposes `data-transfer-input` containing a JSON object with exactly `itemId`, `fromWarehouseId`, `toWarehouseId`, and `quantity`. It must describe a valid 25-unit transfer from East to West for that row.

Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in these attributes.
