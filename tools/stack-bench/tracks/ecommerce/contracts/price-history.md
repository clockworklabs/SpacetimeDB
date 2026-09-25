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

<!-- interface:convex -->
Use `api:admin_change_price` with `{ itemId, price }`.
<!-- /interface -->

<!-- interface:supabase -->
Use the `admin_change_price` PostgreSQL function with `{ itemId, price }`.
<!-- /interface -->

Use the same action as the visible price control.
