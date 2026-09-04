# Promotion rule application interface

Use `promotions-link` for promotion management. Use `promotion-code`, `promotion-discount`,
`promotion-start`, `promotion-end`, `promotion-limit`, and `promotion-submit` to create a rule.
List rules as `promotion-item` elements and expose the saved values with the matching field IDs.

Expose the same rule creation used by `promotion-submit`.

<!-- interface:http -->
Use `POST /api/promotions`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `create_promotion` reducer.
<!-- /interface -->

On a listed rule, `promotion-start` and `promotion-end` show the dates as entered, in ISO
`YYYY-MM-DD` form.
