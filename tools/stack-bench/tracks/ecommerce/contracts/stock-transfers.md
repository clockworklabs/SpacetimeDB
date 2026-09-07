# Stock transfer application interface

Put `transfer-from`, `transfer-to`, `transfer-qty`, and `transfer-submit` inside the applicable
`admin-item-row`. Use `warehouse-total` inside each `admin-warehouse-item` for its numeric stock
total. Show `order-error` when a transfer is refused.

Put `data-transfer-input` on each `admin-item-row`. Its value is a JSON object with exactly
`itemId`, `fromWarehouseId`, and `toWarehouseId` for the currently selected source and destination.
Identifiers can be JSON numbers or strings.

<!-- interface:http -->
Expose `POST /api/admin/transfer`. The JSON body has `itemId`, `fromWarehouseId`,
`toWarehouseId`, and `quantity`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `admin_transfer_stock` with arguments in this order: `itemId: u64`,
`fromWarehouseId: u64`, `toWarehouseId: u64`, `quantity`.
<!-- /interface -->
