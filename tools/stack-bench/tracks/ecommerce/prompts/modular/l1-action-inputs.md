## Purchase action input

Put a `data-buy-input` attribute on each `item-card`. Its value is a JSON
object containing that card's server item identifier, for example
`{"itemId":42}`. The identifier may be a JSON number or string. The runner uses
this test handle to call the same ordinary buy operation as the visible button.

## Restock action input

Put a `data-restock-input` attribute on each `admin-location-row`. Its value is
a JSON object containing the row's server item and warehouse identifiers plus
a valid one-unit restock, for example
`{"itemId":42,"warehouseId":7,"quantity":1}`. Identifiers may be JSON numbers
or strings. The runner uses this test handle to call the same ordinary restock
operation as the visible control.
