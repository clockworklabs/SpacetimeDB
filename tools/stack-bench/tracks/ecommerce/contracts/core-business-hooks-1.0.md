# Core business feature testing interface

## Customer profile hooks

Use `profile-link`, `profile-name`, `profile-address`, and `profile-save` for editing. Show the
saved address in `profile-address-summary`.

## Staff role hooks

Use `staff-role-row`, `staff-role-select`, and `staff-role-save` for role assignment.

## Catalog management hooks

Use `catalog-name`, `catalog-category`, `catalog-price`, `catalog-variants`, and `catalog-save` to
add a product. Use `item-variant` for each variant shown to a visitor.

## Payment record hooks

Use `payment-record`, `payment-status`, and `payment-amount` inside the matching `order-item`.

## Staff activity history hooks

Use `activity-link` and `activity-entry` for staff activity history. Inside each entry, use
`activity-actor`, `activity-action`, and `activity-subject`.
