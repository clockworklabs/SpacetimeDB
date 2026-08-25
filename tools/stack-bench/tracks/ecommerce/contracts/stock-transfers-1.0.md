# Stock transfer testing interface

Put `transfer-from`, `transfer-to`, `transfer-qty`, and `transfer-submit` inside the applicable
`admin-item-row`. Show `order-error` when a transfer is refused.

Put a `data-transfer-input` attribute on the Headphones `admin-item-row`. Its value is a JSON
object with `itemId`, `fromWarehouseId`, `toWarehouseId`, and `quantity`. It must describe a
valid 25-unit transfer from East to West. Identifiers can be JSON numbers or strings.

For direct authorization tests, server-based stacks expose `POST /api/admin/transfer` and
SpacetimeDB exposes `adminTransferStock`. These calls use the same authorization and stock rules
as the visible application.
