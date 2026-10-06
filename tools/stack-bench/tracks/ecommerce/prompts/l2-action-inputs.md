## Operator action inputs

Expose machine-readable inputs for the public application interface. These are inert HTML attributes; they do not prescribe the application framework, database API, or internal implementation.

- Each `order-item` exposes `data-ship-input` containing a JSON object with exactly `orderId`.
- Each `order-item` exposes `data-cancel-input` containing a JSON object with exactly `orderId`.
Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in these attributes.

## Price action input

- Each `admin-item-row` exposes `data-price-input` containing a JSON object with exactly `itemId` and numeric `price` from the current price input.

Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in this attribute.

## Inventory contention action inputs

Expose machine-readable inputs for the public application interface. These are inert HTML attributes; they do not prescribe the application framework, database API, or internal implementation.

- Each catalog `item-card` exposes `data-buy-input` containing a JSON object with exactly `itemId`.
- Each `admin-item-row` exposes `data-transfer-input` containing a JSON object with exactly `itemId`, `fromWarehouseId`, and `toWarehouseId` for the currently selected source and destination.

Use the identifier representation expected by the stack's declared server operation. Do not put credentials or account identifiers in these attributes.
