# Purchasing application interface

Use `catalog-link` to return to the catalog. Use `buy-now` inside an `item-card` to buy one unit. Use `orders-toggle` to open order history.
Use `order-item` for each order, `order-total` for its numeric total, and `order-status` for its
current state. Use `buy-error` or `out-of-stock` for a failed purchase.

Put `data-buy-input` on each `item-card`. Its value is a JSON object containing that item's
server identifier, for example `{"itemId":42}`. Use the same identifier for the visible buy
action.
