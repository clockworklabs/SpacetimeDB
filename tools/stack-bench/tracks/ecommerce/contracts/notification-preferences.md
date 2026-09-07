# Notification preference application interface

Make `notification-settings` available from the catalog while signed in, without first opening another area. Use it to open the settings. Use `notification-order` and
`notification-stock` for the choices, and `notification-save` to save them. Each choice exposes
its current state in `data-state` as `on` or `off`.
Both choices start `off` for a new account. Activating `notification-order` or
`notification-stock` switches it between `on` and `off`.
