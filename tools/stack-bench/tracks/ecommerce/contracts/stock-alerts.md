# Stock alert application interface

Use `stock-alert` inside an unavailable `item-card` to request an alert. Use
`notifications-toggle` to open notifications and `notification-item` for each alert.
On that `item-card`, expose `data-submit-state` for the latest stock-alert request:
`idle` initially, `pending` immediately when submitted, `succeeded` only after the server
confirms success, or `failed` after rejection or transport failure. Keep the terminal state
if the request button disappears. A new submission must replace the old state.
Use `notifications-panel` for the opened notification view, including while its contents
load. Set its `aria-busy` attribute to `false` only when the signed-in account's contents
have loaded successfully, including an empty list; keep it `true` while loading or after
a failed read. Its toggle may also close it.
Within a delivered stock alert, expose `stock-alert-delivery` containing the item's
displayed name. A pending request must not expose `stock-alert-delivery`.

Use `catalog-link` to return to the catalog. If an overlay blocks navigation,
expose a visible `overlay-close` control that dismisses it before navigation.
Screens without a blocking overlay do not need this control.
