## Purchase action input

Put a `data-buy-input` attribute on each `item-card`. Its value is a JSON
object containing that card's server item identifier, for example
`{"itemId":42}`. The identifier may be a JSON number or string. Use this value
with the same ordinary buy operation as the visible button.

## Cart quantity action input

Put a `data-cart-input` attribute on each `cart-item`. Its value is a JSON
object containing that line's server item identifier and the invalid quantity
`-3`, for example `{"itemId":42,"quantity":-3}`. The identifier may be a
JSON number or string. The ordinary cart-quantity operation must refuse the
invalid quantity in this value.

## Restock action input

Put a `data-restock-input` attribute on each `admin-location-row`. Its value is
a JSON object containing the row's server item and warehouse identifiers plus
a valid one-unit restock, for example
`{"itemId":42,"warehouseId":7,"quantity":1}`. Identifiers may be JSON numbers
or strings. Use this value with the same ordinary restock operation as the
visible control.
