# Order-linked support application interface

## Order-linked support hooks

Within a `support-ticket`, use `support-order-option` for each order that the customer can attach,
`support-link-order` to attach the selected order, and `support-order` for the attached order. The
link action must expose its input in `data-action-input` for the named
`linkSupportOrder` application action.

For HTTP stacks, `linkSupportOrder` is `POST /api/support/cases/{caseId}/order`. For reducer
stacks, it is `linkSupportOrder(caseId, orderId)`.
