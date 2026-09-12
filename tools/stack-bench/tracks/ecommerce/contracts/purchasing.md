# Purchasing application interface

Use `catalog-link` to return to the catalog. Use `buy-now` inside an `item-card` to buy one unit. Use `orders-toggle` to open order history.
If an overlay blocks catalog navigation, expose a visible `overlay-close` control
that dismisses it before `catalog-link` is used. Screens without a blocking
overlay need no such control. Dialogs, panels, and ordinary page layouts are all allowed.
Use `order-item` for each order, containing the names of its purchased items. Inside that
`order-item`, use `order-total` for its numeric total and `order-status` for its current state. Show `out-of-stock` inside an `item-card` once that item's stock reaches zero.
Use `buy-error` for a failed purchase.

Put `data-buy-input` on each `item-card`. Its value is a JSON object containing that item's
server identifier, for example `{"itemId":42}`. The identifier may be a JSON number or string.
Use the same identifier for the visible buy action.

Expose the same purchase used by `buy-now`.

<!-- interface:http -->
Use `POST /api/items/:id/buy`, where `:id` is the item identifier.
<!-- /interface -->

<!-- interface:reducer -->
Use the `buy_now` reducer with the item identifier.
<!-- /interface -->
