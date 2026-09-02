# Cart application interface

Use `catalog-link` to return to the catalog. Use `add-to-cart` inside an `item-card` to add one unit. Use `cart-toggle` to open the cart.
Use `cart-count` for the total units, `cart-item` for each line, `cart-quantity` for its
quantity, and `cart-total` for the numeric total. Use `cart-remove` to remove a line and
`empty-cart` for an empty cart.

Put `data-buy-input` on each `item-card`. Its value is a JSON object containing that item's
server identifier, for example `{"itemId":42}`. Put `data-cart-input` on each `cart-item`.
Its value contains the item identifier and the invalid quantity `-3`, for example
`{"itemId":42,"quantity":-3}`. The cart quantity action must reject this value.

Use `checkout-submit` to check out. Use `orders-toggle` to open order history and `order-item`
for each order created by checkout. Cart state must remain correct after reconnect and reload.
