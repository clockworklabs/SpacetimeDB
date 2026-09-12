# Managed support application interface

## Managed support controls

Use `support-ticket` for each case and set `data-entity-id` to that case's server identifier.
Within a case, use `support-status` for the current status, `support-reply` for the reply field,
`support-reply-submit` to send a reply, and `support-reply-item` for each reply.

Expose the same reply operation used by `support-reply-submit`.

<!-- interface:http -->
Use `POST /api/support/:id/replies`, where `:id` is the case identifier from `data-entity-id`.
The JSON body is `{ "body": "<reply text>" }`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `reply_support` reducer with arguments in this order: `ticketId: u64`, `body: string`.
Render `data-entity-id` as the decimal case identifier without precision loss.
<!-- /interface -->
