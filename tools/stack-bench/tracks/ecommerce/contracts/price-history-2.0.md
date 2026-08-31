# Price history application interface

Put `price-input` and `price-submit` inside the applicable `admin-item-row`.

Put a `data-price-input` attribute on each `admin-item-row`. Its value is a JSON object with
`itemId` and numeric `price`. The Gaming Mouse row uses `1.00` as the price. Identifiers can be
JSON numbers or strings.

For direct authorization actions, server-based stacks expose `POST /api/admin/price` and
SpacetimeDB exposes `adminChangePrice`. These calls use the same authorization and price rules as
the visible application.

The price-history checks also use `add-to-cart` inside an `item-card`, `cart-toggle`, `cart-total`,
and `checkout-submit`. Use `orders-toggle`, `order-item`, and `order-total` to inspect the order
created at checkout. Use `buy-now` inside an `item-card` to create a paid order before a price
change.
