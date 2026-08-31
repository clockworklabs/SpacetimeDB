# Cart application interface

Use `add-to-cart` inside an `item-card` to add one unit. Use `cart-toggle` to open the cart.
Use `cart-count` for the total units, `cart-item` for each line, `cart-quantity` for its
quantity, and `cart-total` for the numeric total. Use `cart-remove` to remove a line and
`empty-cart` for an empty cart.

Use `checkout-submit` to check out. Use `orders-toggle` to open order history and `order-item`
for each order created by checkout. Cart state must remain correct after reconnect and reload.
