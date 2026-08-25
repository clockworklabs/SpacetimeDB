# Price history testing interface

Put `price-input` and `price-submit` inside the applicable `admin-item-row`.

Put a `data-price-input` attribute on each `admin-item-row`. Its value is a JSON object with
`itemId` and numeric `price`. The Gaming Mouse row uses `1.00` as the price. Identifiers can be
JSON numbers or strings.

For direct authorization tests, server-based stacks expose `POST /api/admin/price` and
SpacetimeDB exposes `adminChangePrice`. These calls use the same authorization and price rules as
the visible application.
