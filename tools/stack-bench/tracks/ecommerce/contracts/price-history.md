# Price history application interface

Put `price-input` and `price-submit` inside the applicable `admin-item-row`.

Put a `data-price-input` attribute on each `admin-item-row`. Its value is a JSON object with
`itemId` and numeric `price` from the current price input. Identifiers can be
JSON numbers or strings.

<!-- interface:http -->
Expose `POST /api/admin/price`.
<!-- /interface -->

<!-- interface:reducer -->
Expose the `admin_change_price` reducer.
<!-- /interface -->

This action uses the same authorization and price rules as the visible application.
