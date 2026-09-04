# Stock transfer application interface

Put `transfer-from`, `transfer-to`, `transfer-qty`, and `transfer-submit` inside the applicable
`admin-item-row`. Use `warehouse-total` inside each `admin-warehouse-item` for its numeric stock
total. Show `order-error` when a transfer is refused.

Put `data-transfer-input` on each `admin-item-row`. Its value is a JSON object with `itemId`,
`fromWarehouseId`, and `toWarehouseId`. Identifiers can be JSON numbers or strings.

<!-- interface:http -->
Expose `POST /api/admin/transfer`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `admin_transfer_stock`.
<!-- /interface -->
