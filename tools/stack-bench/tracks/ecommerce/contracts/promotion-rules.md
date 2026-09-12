# Promotion rule application interface

Use `staff-link` to open the staff area. In that area, use `promotions-link` for promotion
management. Use `promotion-code`, `promotion-discount`,
`promotion-start`, `promotion-end`, `promotion-limit`, and `promotion-submit` to create a rule.
List rules as `promotion-item` elements and expose the saved values with the matching field IDs.

Expose the same rule creation used by `promotion-submit`.

<!-- interface:http -->
Use `POST /api/promotions` with a JSON object containing `code` (string),
`discountPercent` (number), `startMicros` and `endMicros` (integer numbers of microseconds
since the Unix epoch), and `usageLimit` (positive integer).
<!-- /interface -->

<!-- interface:reducer -->
Use the `create_promotion` reducer with arguments in this order: `code: string`,
`discountPercent: f64`, `startMicros: i64`, `endMicros: i64`, `usageLimit: u32`.
Both time arguments are microseconds since the Unix epoch.
<!-- /interface -->

On a listed rule, `promotion-start` and `promotion-end` show the dates as entered, in ISO
`YYYY-MM-DD` form.
