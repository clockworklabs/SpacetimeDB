# Managed support application interface

## Managed support controls

Use `support-ticket` for each case. Within a case, use `support-status` for the current status,
`support-reply` for the reply field, `support-reply-submit` to send a reply, and
`support-reply-item` for each reply.

Expose the same reply operation used by `support-reply-submit`.

<!-- interface:http -->
Use `POST /api/support/:id/replies`.
<!-- /interface -->

<!-- interface:reducer -->
Use the `reply_support` reducer.
<!-- /interface -->
