# Support refund application interface

## Support refund controls

Within a `support-ticket`, use `support-refund` for the refund action and
`support-refund-total` for the recorded refund amount. The refund action must expose its input in
`data-action-input` for the named `supportRefund` application action. Within an `order-item`, use
`order-refund-total` for the refunded amount and `refund-entry` for each refund record. Each
`refund-entry` includes the order item name.

For HTTP stacks, `supportRefund` is `POST /api/support/cases/{caseId}/refund`. For reducer stacks,
it is `supportRefund(caseId)`.
