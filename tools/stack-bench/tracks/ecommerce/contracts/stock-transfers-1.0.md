# Stock transfer application interface

Put `transfer-from`, `transfer-to`, `transfer-qty`, and `transfer-submit` inside the applicable
`admin-item-row`. Use `warehouse-total` inside each `admin-warehouse-item` for its numeric stock
total. Show `order-error` when a transfer is refused.

Put a `data-transfer-input` attribute on the Headphones `admin-item-row`. Its value is a JSON
object with `itemId`, `fromWarehouseId`, `toWarehouseId`, and `quantity`. It must describe a
valid 25-unit transfer from East to West. Identifiers can be JSON numbers or strings.

The transfer action uses the same authorization and stock rules as the visible application.

<!-- interface:http -->
Expose `POST /api/admin/transfer`.
<!-- /interface -->

<!-- interface:reducer -->
Expose `adminTransferStock`.
<!-- /interface -->
