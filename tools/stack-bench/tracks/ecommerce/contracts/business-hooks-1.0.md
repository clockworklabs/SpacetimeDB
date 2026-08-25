# Business feature testing interface

## Staff access hooks

Use `staff-signin-username`, `staff-signin-password`, and `staff-signin-submit` for sign-in. Show
the active account in `staff-current-user`. Use `staff-link` for the staff area and `admin-link`
for the administrator area.

The test fixture signs in with these accounts:

- staff: `staff` / `stackbench-staff-2026`
- administrator: `admin` / `stackbench-admin-2026`
- customer: `customer` / `stackbench-customer-2026`

## Support hooks

Use `support-link`, `support-email`, `support-subject`, `support-message`, `support-submit`,
`support-reference`, `support-ticket`, `support-status`, `support-priority`, `support-assignee`,
`support-assign`, `support-reply`, `support-reply-submit`, `support-order`, `support-link-order`,
and `support-refund`.

## Promotion hooks

Use `promotions-link`, `promotion-code`, `promotion-discount`, `promotion-start`,
`promotion-end`, `promotion-limit`, `promotion-submit`, `cart-promotion`,
`apply-promotion`, `order-discount`, `promotion-report`, `promotion-redemptions`, and
`promotion-revenue`.

## Notification hooks

Use `notification-settings`, `notification-order`, `notification-stock`,
`notification-save`, `stock-alert`, `notifications-toggle`, and `notification-item`.
