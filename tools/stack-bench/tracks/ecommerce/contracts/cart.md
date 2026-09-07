# Cart application interface

Use `catalog-link` to return to the catalog. Use `add-to-cart` inside an `item-card` to add one unit. Use `cart-toggle` to open the cart.
If an overlay blocks catalog navigation, expose a visible `overlay-close` control
that dismisses it before `catalog-link` is used. Screens without a blocking
overlay need no such control. Dialogs, panels, and ordinary page layouts are all allowed.
Use `cart-count` for the total units, `cart-item` for each line, `cart-quantity` for its
quantity, and `cart-total` for the numeric total. Use `cart-remove` to remove a line and
`empty-cart` for an empty cart. Keep `cart-count` visible, showing 0, while the cart is empty.

Put `data-buy-input` on each `item-card`. Its value is a JSON object containing that item's
server identifier, for example `{"itemId":42}`. Put `data-cart-input` on each `cart-item`.
Its value contains the item identifier, for example `{"itemId":42}`. The identifier may be a
JSON number or string.

Expose the same add and quantity-update operations used by the cart controls.

<!-- interface:http -->
Use `POST /api/cart`. Put `itemId` in the JSON body.
Use `PATCH /api/cart/:itemId`. Put `quantity` in the JSON body.
<!-- /interface -->

<!-- interface:reducer -->
Use the `add_to_cart` reducer with the item identifier.
Use the `update_cart_quantity` reducer with the item identifier and quantity.
<!-- /interface -->

Use `checkout-submit` to check out. Use `orders-toggle` to open order history and `order-item`
for each order created by checkout.
