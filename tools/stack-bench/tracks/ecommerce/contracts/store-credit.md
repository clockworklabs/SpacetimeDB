# Store credit interface

Open customer credit with `credit-link`. The `credit-panel` exposes the signed-in account's `data-account-id` and `aria-busy="false"` when loaded. Show `credit-balance` in major currency units and one `credit-entry` per movement.

Staff use `credit-customer`, `credit-amount-input` (major units), `credit-reference-input`, and `credit-grant`. The grant control exposes `data-action-input` as JSON with `accountId`, `amountMinor`, and `reference`.

Use `credit-checkout` in the cart. Each `order-item` shows `payment-credit-amount` and `payment-external-amount` in major units. Their sum is `payment-amount`.

<!-- interface:http -->
`grantCredit` is `POST /api/staff/credit` with `accountId`, `amountMinor`, and `reference`.
`checkoutCredit` is `POST /api/checkout/credit` with no body fields.
<!-- /interface -->

<!-- interface:reducer -->
`grantCredit` is `grant_credit(accountId, amountMinor, reference)`.
`checkoutCredit` is `checkout_credit()`.
<!-- /interface -->
