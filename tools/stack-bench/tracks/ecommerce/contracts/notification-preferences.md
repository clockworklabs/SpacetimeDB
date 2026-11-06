# Notification preference application interface

Saving may leave settings open. Use `catalog-link` to return to the catalog. If
a settings overlay blocks navigation, expose `overlay-close` to dismiss it.

While signed in on the catalog, show `notification-settings` directly or reveal it by clicking
`current-user`. No other navigation is required to reach it. Use it to open the settings.
Use `notification-order` and
`notification-stock` for the choices, and `notification-save` to save them. Each choice exposes
its current state in `data-state` as `on` or `off`.
Both choices start `off` for a new account. Activating `notification-order` or
`notification-stock` switches it between `on` and `off`.

Expose `notification-save-state` as the status of the latest save, with `data-submit-state`:
`idle` initially, `pending` immediately when submitted, `succeeded` only after the
server confirms success, or `failed` after rejection or transport failure. Keep this
status visible if the editor closes. Keep its terminal state until the next submission
or page navigation. A new submission must replace the old state immediately.
